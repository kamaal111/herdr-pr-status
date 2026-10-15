use crate::{
    github::{self, Checkout},
    herdr,
};
use anyhow::{Context, Result, bail, ensure};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions, TryLockError},
    os::unix::{
        fs::{MetadataExt, OpenOptionsExt},
        process::CommandExt,
    },
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub struct Session {
    directory: PathBuf,
    socket: PathBuf,
    identity: (u64, u64),
}

impl Session {
    pub fn load() -> Result<Self> {
        let socket = PathBuf::from(
            std::env::var_os("HERDR_SOCKET_PATH")
                .context("HERDR_SOCKET_PATH is required; run through a Herdr plugin action")?,
        );
        ensure!(socket.is_absolute(), "HERDR_SOCKET_PATH must be absolute");
        let metadata = fs::metadata(&socket).context("inspect Herdr session socket")?;
        let state = PathBuf::from(
            std::env::var_os("HERDR_PLUGIN_STATE_DIR")
                .context("HERDR_PLUGIN_STATE_DIR is required")?,
        );
        ensure!(
            state.is_absolute(),
            "HERDR_PLUGIN_STATE_DIR must be absolute"
        );
        let hash = socket
            .as_os_str()
            .as_encoded_bytes()
            .iter()
            .fold(0xcbf29ce484222325_u64, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
            });
        Ok(Self {
            directory: state.join(format!(
                "session-{hash:016x}-{:x}-{:x}",
                metadata.dev(),
                metadata.ino()
            )),
            socket,
            identity: (metadata.dev(), metadata.ino()),
        })
    }

    fn same_session(&self) -> bool {
        fs::metadata(&self.socket).is_ok_and(|m| (m.dev(), m.ino()) == self.identity)
    }

    fn stopped(&self) -> bool {
        self.directory.join("stop").exists() || !self.same_session()
    }

    fn file(&self, name: &str) -> Result<File> {
        fs::create_dir_all(&self.directory)?;
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(self.directory.join(name))
            .context("open session lock/log")
    }

    fn lock(&self, name: &str) -> Result<Option<File>> {
        let file = self.file(name)?;
        match file.try_lock() {
            Ok(()) => Ok(Some(file)),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn start(&self) -> Result<()> {
        let _control = self
            .lock("control.lock")?
            .context("another start/stop is in progress")?;
        let restart = self.directory.join("stop").exists();
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if let Some(probe) = self.lock("watch.lock")? {
                drop(probe);
                break;
            }
            if !restart {
                println!("PR watcher is already running");
                return Ok(());
            }
            ensure!(
                Instant::now() < deadline,
                "previous watcher is still stopping; retry start shortly"
            );
            thread::sleep(Duration::from_millis(20));
        }
        match fs::remove_file(self.directory.join("stop")) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        ensure!(herdr::enabled()?, "plugin is not installed and enabled");
        let log = OpenOptions::new()
            .append(true)
            .create(true)
            .mode(0o600)
            .open(self.directory.join("watch.log"))?;
        let mut command = Command::new(std::env::current_exe()?);
        command
            .arg("watch")
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log));
        // SAFETY: setsid has no pointer arguments and is async-signal-safe.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = command.spawn().context("launch PR watcher")?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if self.lock("watch.lock")?.is_none() {
                println!(
                    "PR watcher started; diagnostics: {}",
                    self.directory.join("watch.log").display()
                );
                return Ok(());
            }
            if let Some(status) = child.try_wait()? {
                bail!("PR watcher exited ({status}); inspect watch.log");
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                bail!("PR watcher did not start within five seconds");
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    pub fn stop(&self) -> Result<()> {
        let _control = self
            .lock("control.lock")?
            .context("another start/stop is in progress")?;
        fs::write(self.directory.join("stop"), [])?;
        println!("Stop requested; PR tokens expire within 180 seconds");
        Ok(())
    }

    pub fn watch(&self) -> Result<()> {
        let Some(_watch) = self.lock("watch.lock")? else {
            return Ok(());
        };
        eprintln!("PR watcher started");
        let mut refresh_at = Instant::now();
        while !self.stopped() {
            match herdr::enabled() {
                Ok(true) => {}
                Ok(false) => break,
                Err(error) => {
                    eprintln!("PR watcher lost session: {error:#}");
                    break;
                }
            }
            if Instant::now() >= refresh_at {
                if let Err(error) = self.refresh(false) {
                    eprintln!("PR refresh failed: {error:#}");
                }
                refresh_at = Instant::now() + Duration::from_secs(60);
            }
            for _ in 0..5 {
                if self.stopped() {
                    break;
                }
                thread::sleep(Duration::from_secs(1));
            }
        }
        eprintln!("PR watcher stopped");
        Ok(())
    }

    pub fn refresh(&self, manual: bool) -> Result<()> {
        let Some(_refresh) = self.lock("refresh.lock")? else {
            if manual {
                println!("A PR refresh is already in progress");
            }
            return Ok(());
        };
        let snapshot = herdr::snapshot()?;
        let mut cache: HashMap<Checkout, std::result::Result<Option<u64>, String>> = HashMap::new();
        let mut failures = 0;
        let mut reported = 0;
        for workspace in &snapshot.workspaces {
            if !self.same_session() || (!manual && (self.stopped() || !herdr::enabled()?)) {
                break;
            }
            let result = (|| -> Result<()> {
                let original = snapshot
                    .directory(workspace)
                    .map(github::checkout)
                    .transpose()?
                    .flatten();
                let number = match &original {
                    Some(checkout) => cache
                        .entry(checkout.clone())
                        .or_insert_with(|| {
                            github::lookup(checkout).map_err(|error| format!("{error:#}"))
                        })
                        .clone()
                        .map_err(anyhow::Error::msg)?,
                    None => None,
                };
                let fresh = herdr::snapshot()?;
                let Some(current) = fresh
                    .workspaces
                    .iter()
                    .find(|w| w.workspace_id == workspace.workspace_id)
                else {
                    return Ok(());
                };
                let current_checkout = fresh
                    .directory(current)
                    .map(github::checkout)
                    .transpose()?
                    .flatten();
                if !self.same_session() || (!manual && (self.stopped() || !herdr::enabled()?)) {
                    return Ok(());
                }
                let number = if original == current_checkout {
                    number
                } else {
                    None
                };
                herdr::report(&workspace.workspace_id, number)?;
                reported += 1;
                Ok(())
            })();
            if let Err(error) = result {
                failures += 1;
                eprintln!("workspace {}: {error:#}", workspace.workspace_id);
            }
        }
        eprintln!("PR refresh: {reported} workspace reports, {failures} failures");
        ensure!(
            failures == 0,
            "{failures} workspace PR lookups/reports failed"
        );
        Ok(())
    }
}

pub fn check() -> Result<()> {
    let snapshot = herdr::snapshot()?;
    let mut cache = HashMap::new();
    for workspace in &snapshot.workspaces {
        let checkout = snapshot
            .directory(workspace)
            .map(github::checkout)
            .transpose()?
            .flatten();
        let number = match checkout {
            Some(checkout) => {
                if let Some(number) = cache.get(&checkout) {
                    *number
                } else {
                    let number = github::lookup(&checkout)?;
                    cache.insert(checkout, number);
                    number
                }
            }
            None => None,
        };
        println!(
            "{}: {}",
            workspace.workspace_id,
            number
                .map(|n| format!("#{n}"))
                .unwrap_or_else(|| "no associated PR".into())
        );
    }
    Ok(())
}
