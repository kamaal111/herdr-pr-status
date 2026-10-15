use crate::command::run;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Checkout {
    pub root: PathBuf,
    pub branch: String,
    pub head_repo: String,
    pub head_branch: String,
    pub host: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Remote {
    pub host: String,
    pub repository: String,
}

pub fn parse_remote(value: &str) -> Option<Remote> {
    let value = value.trim();
    let (host, path) = if let Some(rest) = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .or_else(|| value.strip_prefix("ssh://"))
    {
        let (authority, path) = rest.split_once('/')?;
        (authority.rsplit('@').next()?.split(':').next()?, path)
    } else {
        let (authority, path) = value.split_once(':')?;
        (authority.rsplit('@').next()?, path)
    };
    let path = path
        .trim_end_matches('/')
        .strip_suffix(".git")
        .unwrap_or(path.trim_end_matches('/'));
    let parts: Vec<_> = path.split('/').collect();
    if host.is_empty() || parts.len() != 2 || parts.iter().any(|s| s.is_empty()) {
        return None;
    }
    Some(Remote {
        host: host.to_owned(),
        repository: path.to_owned(),
    })
}

fn git(directory: &Path, args: &[&str]) -> Result<String> {
    let output = run(
        Command::new("git")
            .env("LC_ALL", "C")
            .arg("-C")
            .arg(directory)
            .args(args),
        Duration::from_secs(5),
    )?;
    Ok(String::from_utf8(output)
        .context("Git output is not UTF-8")?
        .trim()
        .to_owned())
}

pub fn checkout(directory: &Path) -> Result<Option<Checkout>> {
    if !directory.is_absolute() || !directory.is_dir() {
        return Ok(None);
    }
    // --is-inside-work-tree distinguishes ordinary directories without treating
    // authentication or network errors as evidence that a PR does not exist.
    let inside = git(directory, &["rev-parse", "--is-inside-work-tree"]);
    match inside {
        Ok(value) if value == "true" => {}
        Ok(_) => return Ok(None),
        Err(error) if error.to_string().contains("not a git repository") => return Ok(None),
        Err(error) => return Err(error),
    }
    let root = PathBuf::from(git(directory, &["rev-parse", "--show-toplevel"])?);
    let branch = git(&root, &["branch", "--show-current"])?;
    if branch.is_empty() {
        return Ok(None);
    }
    let remote_name = git(
        &root,
        &["config", "--get", &format!("branch.{branch}.remote")],
    )
    .ok()
    .filter(|s| !s.is_empty() && s != ".")
    .unwrap_or_else(|| "origin".into());
    let remote_url = git(&root, &["remote", "get-url", &remote_name]).unwrap_or_default();
    let Some(remote) = parse_remote(&remote_url) else {
        return Ok(None);
    };
    let head_branch = git(
        &root,
        &["config", "--get", &format!("branch.{branch}.merge")],
    )
    .ok()
    .and_then(|s| s.strip_prefix("refs/heads/").map(str::to_owned))
    .unwrap_or_else(|| branch.clone());
    Ok(Some(Checkout {
        root,
        branch,
        head_branch,
        head_repo: remote.repository,
        host: remote.host,
    }))
}

#[derive(Deserialize)]
struct Repository {
    #[serde(rename = "nameWithOwner")]
    name_with_owner: String,
    url: String,
}

#[derive(Deserialize)]
pub struct PullRequest {
    number: u64,
    created_at: String,
    head: Head,
}
#[derive(Deserialize)]
struct Head {
    #[serde(rename = "ref")]
    branch: String,
    repo: Option<HeadRepository>,
}
#[derive(Deserialize)]
struct HeadRepository {
    full_name: String,
}

pub fn newest_pr(bytes: &[u8], checkout: &Checkout) -> Result<Option<u64>> {
    let pages = serde_json::Deserializer::from_slice(bytes).into_iter::<Vec<PullRequest>>();
    let mut newest: Option<PullRequest> = None;
    for page in pages {
        for pr in page.context("parse GitHub PR response")? {
            if pr.head.branch == checkout.head_branch
                && pr
                    .head
                    .repo
                    .as_ref()
                    .is_some_and(|r| r.full_name.eq_ignore_ascii_case(&checkout.head_repo))
            {
                ensure!(
                    pr.number > 0 && !pr.created_at.is_empty(),
                    "invalid GitHub PR number/date"
                );
                if newest
                    .as_ref()
                    .is_none_or(|old| (&pr.created_at, pr.number) > (&old.created_at, old.number))
                {
                    newest = Some(pr);
                }
            }
        }
    }
    ensure!(
        !bytes.iter().all(u8::is_ascii_whitespace),
        "empty GitHub response"
    );
    Ok(newest.map(|pr| pr.number))
}

fn gh(checkout: &Checkout) -> Command {
    let mut command = Command::new("gh");
    command
        .current_dir(&checkout.root)
        .env_remove("GH_REPO")
        .env("GH_HOST", &checkout.host)
        .env("GH_PROMPT_DISABLED", "1");
    command
}

pub fn lookup(checkout: &Checkout) -> Result<Option<u64>> {
    let repository: Repository = serde_json::from_slice(&run(
        gh(checkout).args(["repo", "view", "--json", "nameWithOwner,url"]),
        Duration::from_secs(15),
    )?)
    .context("resolve GitHub base repository")?;
    let base = parse_remote(&repository.url).context("invalid GitHub repository URL")?;
    ensure!(
        base.host.eq_ignore_ascii_case(&checkout.host),
        "GitHub base/head hosts differ"
    );
    ensure!(
        base.repository
            .eq_ignore_ascii_case(&repository.name_with_owner),
        "GitHub repository identity mismatch"
    );
    let (owner, _) = checkout
        .head_repo
        .split_once('/')
        .context("invalid head repository")?;
    let endpoint = format!("repos/{}/pulls", repository.name_with_owner);
    let head = format!("head={owner}:{}", checkout.head_branch);
    let output = run(
        gh(checkout).args([
            "api",
            "--hostname",
            &checkout.host,
            "--method",
            "GET",
            &endpoint,
            "-f",
            "state=all",
            "-f",
            &head,
            "-f",
            "sort=created",
            "-f",
            "direction=desc",
            "-f",
            "per_page=100",
            "--paginate",
        ]),
        Duration::from_secs(20),
    )?;
    newest_pr(&output, checkout)
}
