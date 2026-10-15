use anyhow::{Result, bail};
use herdr_pr_status::watcher::{self, Session};

fn execute() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let action = args.next().unwrap_or_else(|| "--help".into());
    if args.next().is_some() {
        bail!("expected a single command; use --help");
    }
    match action.as_str() {
        "start" => Session::load()?.start(),
        "watch" => Session::load()?.watch(),
        "refresh" => Session::load()?.refresh(true),
        "stop" => Session::load()?.stop(),
        "check" => watcher::check(),
        "--version" | "-V" => {
            println!("herdr-pr-status {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "--help" | "-h" => {
            println!(
                "herdr-pr-status\n\nstart    Start one background watcher per Herdr session\nrefresh  Refresh workspace PR metadata once\nstop     Request watcher shutdown\ncheck    Read PR associations without reporting metadata\nwatch    Internal watcher entrypoint\n\nRequires git and authenticated gh. No local Herdr settings are edited."
            );
            Ok(())
        }
        _ => bail!("unknown command {action}; use --help"),
    }
}

fn main() {
    if let Err(error) = execute() {
        eprintln!("herdr-pr-status: {error:#}");
        std::process::exit(1);
    }
}
