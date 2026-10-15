use crate::command::run;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

pub const SOURCE: &str = "herdr-pr-status";
pub const TOKEN: &str = "pr_number";

#[derive(Debug, Deserialize)]
pub struct Workspace {
    pub workspace_id: String,
    pub active_tab_id: String,
    pub worktree: Option<Worktree>,
}

#[derive(Debug, Deserialize)]
pub struct Worktree {
    pub checkout_path: PathBuf,
}

#[derive(Debug, Deserialize)]
pub struct Pane {
    pub pane_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub cwd: Option<PathBuf>,
    pub foreground_cwd: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
pub struct Layout {
    pub workspace_id: String,
    pub tab_id: String,
    pub focused_pane_id: String,
}

#[derive(Debug, Deserialize)]
pub struct Snapshot {
    pub workspaces: Vec<Workspace>,
    pub panes: Vec<Pane>,
    pub layouts: Vec<Layout>,
}

impl Snapshot {
    pub fn directory<'a>(&'a self, workspace: &'a Workspace) -> Option<&'a Path> {
        if let Some(worktree) = &workspace.worktree {
            return Some(worktree.checkout_path.as_path());
        }
        let layout = self.layouts.iter().find(|layout| {
            layout.workspace_id == workspace.workspace_id
                && layout.tab_id == workspace.active_tab_id
        })?;
        let pane = self.panes.iter().find(|pane| {
            pane.pane_id == layout.focused_pane_id
                && pane.workspace_id == workspace.workspace_id
                && pane.tab_id == workspace.active_tab_id
        })?;
        pane.foreground_cwd.as_deref().or(pane.cwd.as_deref())
    }

    fn validate(&self) -> Result<()> {
        let mut ids = HashSet::new();
        for workspace in &self.workspaces {
            ensure!(
                !workspace.workspace_id.trim().is_empty(),
                "empty workspace ID"
            );
            ensure!(
                ids.insert(&workspace.workspace_id),
                "duplicate workspace ID"
            );
        }
        Ok(())
    }
}

#[derive(Deserialize)]
struct Envelope {
    result: SnapshotResult,
}
#[derive(Deserialize)]
struct SnapshotResult {
    #[serde(rename = "type")]
    kind: String,
    snapshot: Snapshot,
}

#[derive(Deserialize)]
struct PluginEnvelope {
    result: PluginResult,
}
#[derive(Deserialize)]
struct PluginResult {
    plugins: Vec<Plugin>,
}
#[derive(Deserialize)]
struct Plugin {
    plugin_id: String,
    enabled: bool,
}

pub fn cli(args: &[&str]) -> Result<Vec<u8>> {
    run(
        Command::new(std::env::var_os("HERDR_BIN_PATH").unwrap_or_else(|| "herdr".into()))
            .args(args),
        Duration::from_secs(5),
    )
}

pub fn snapshot() -> Result<Snapshot> {
    let envelope: Envelope =
        serde_json::from_slice(&cli(&["api", "snapshot"])?).context("parse Herdr snapshot")?;
    ensure!(
        envelope.result.kind == "session_snapshot",
        "unexpected Herdr snapshot response"
    );
    envelope.result.snapshot.validate()?;
    Ok(envelope.result.snapshot)
}

pub fn enabled() -> Result<bool> {
    let envelope: PluginEnvelope = serde_json::from_slice(&cli(&["plugin", "list", "--json"])?)
        .context("parse Herdr plugin list")?;
    Ok(envelope
        .result
        .plugins
        .iter()
        .any(|p| p.plugin_id == SOURCE && p.enabled))
}

pub fn report(workspace_id: &str, number: Option<u64>) -> Result<()> {
    let value = number.map(|n| format!("{TOKEN}=#{n}"));
    let mut args = vec![
        "workspace",
        "report-metadata",
        workspace_id,
        "--source",
        SOURCE,
    ];
    if let Some(value) = &value {
        args.extend(["--token", value, "--ttl-ms", "180000"]);
    } else {
        args.extend(["--clear-token", TOKEN]);
    }
    let response: serde_json::Value =
        serde_json::from_slice(&cli(&args)?).context("parse metadata report response")?;
    ensure!(
        response.get("result").is_some() && response.get("error").is_none(),
        "metadata report rejected: {response}"
    );
    Ok(())
}
