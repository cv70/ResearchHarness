use std::{
    fs::File,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

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
    pub log_path: PathBuf,
    pub timed_out: bool,
}

pub fn run_command(
    workspace_root: impl AsRef<Path>,
    command: &ExperimentCommand,
) -> Result<CommandResult> {
    let started = Instant::now();
    let log_path = if command.log_path.is_absolute() {
        command.log_path.clone()
    } else {
        workspace_root.as_ref().join(&command.log_path)
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

    let timeout = Duration::from_secs(command.timeout_seconds);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(CommandResult {
                exit_code: status.code(),
                duration: started.elapsed(),
                log_path: log_path.clone(),
                timed_out: false,
            });
        }
        if started.elapsed() >= timeout {
            child.kill()?;
            let _ = child.wait();
            return Ok(CommandResult {
                exit_code: None,
                duration: started.elapsed(),
                log_path: log_path.clone(),
                timed_out: true,
            });
        }
        thread::sleep(Duration::from_millis(100));
    }
}

impl CommandResult {
    pub fn ensure_success(&self) -> Result<()> {
        if self.timed_out {
            return Err(HarnessError::Experiment(format!(
                "command timed out after {:.1}s",
                self.duration.as_secs_f64()
            )));
        }
        match self.exit_code {
            Some(0) => Ok(()),
            other => Err(HarnessError::Experiment(format!(
                "command exited with {other:?}"
            ))),
        }
    }
}
