use std::{
    fmt::Write as _,
    fs::File,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use wait_timeout::ChildExt;

use crate::core::{HarnessError, Result};

#[derive(Debug, Clone)]
pub struct ExperimentCommand {
    pub command: String,
    pub timeout_seconds: u64,
    pub log_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct CommandResult {
    pub exit_code: Option<i32>,
    pub duration: Duration,
    pub timed_out: bool,
}

pub fn run_command(
    workspace_root: impl AsRef<Path>,
    command: &ExperimentCommand,
) -> Result<CommandResult> {
    let workspace_root = workspace_root.as_ref();
    let started = Instant::now();
    let log_path = if command.log_path.is_absolute() {
        command.log_path.clone()
    } else {
        workspace_root.join(&command.log_path)
    };
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let log = File::create(&log_path)?;
    let log_err = log.try_clone()?;
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(&command.command)
        .current_dir(workspace_root)
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err))
        .spawn()?;

    // Guard ensures child is reaped even if wait_timeout returns an I/O error.
    struct ChildGuard<'a>(&'a mut std::process::Child);
    impl Drop for ChildGuard<'_> {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let guard = ChildGuard(&mut child);

    let timeout = Duration::from_secs(command.timeout_seconds);
    let result = match guard.0.wait_timeout(timeout)? {
        Some(status) => Ok(CommandResult {
            exit_code: status.code(),
            duration: started.elapsed(),
            timed_out: false,
        }),
        None => {
            let _ = guard.0.kill();
            let _ = guard.0.wait();
            Ok(CommandResult {
                exit_code: None,
                duration: started.elapsed(),
                timed_out: true,
            })
        }
    };
    std::mem::forget(guard);
    result
}

impl CommandResult {
    pub fn ensure_success(&self) -> Result<()> {
        if self.timed_out {
            let mut msg = String::with_capacity(32);
            write!(
                msg,
                "command timed out after {:.1}s",
                self.duration.as_secs_f64()
            )
            .unwrap();
            return Err(HarnessError::Experiment(msg));
        }
        match self.exit_code {
            Some(0) => Ok(()),
            other => {
                let mut msg = String::with_capacity(32);
                write!(msg, "command exited with {other:?}").unwrap();
                Err(HarnessError::Experiment(msg))
            }
        }
    }
}
