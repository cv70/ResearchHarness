use std::{
    fmt::Write as _,
    io::Write,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use wait_timeout::ChildExt;

use crate::{
    agents::{AgentRequest, AgentResponse, AgentRunner},
    core::{HarnessError, Result},
};

#[derive(Debug, Clone)]
pub struct CliAgentRunner {
    program: String,
    args: Vec<String>,
}

impl CliAgentRunner {
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
        }
    }
}

impl AgentRunner for CliAgentRunner {
    fn run(&self, request: &AgentRequest) -> Result<AgentResponse> {
        let started = Instant::now();
        let mut child = Command::new(&self.program)
            .args(&self.args)
            .current_dir(&request.working_directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            let write_result = (|| {
                stdin.write_all(request.system_prompt.as_bytes())?;
                stdin.write_all(b"\n\n")?;
                stdin.write_all(request.task_prompt.as_bytes())?;
                stdin.flush()
            })();
            if write_result.is_err() {
                let _ = child.kill();
                let _ = child.wait();
                let mut msg = String::with_capacity(self.program.len() + 40);
                let _ = write!(msg, "{} closed stdin before receiving prompt", self.program);
                return Err(HarnessError::Agent(msg));
            }
        }

        let timeout = Duration::from_secs(request.timeout_seconds);
        if child.wait_timeout(timeout)?.is_none() {
            let _ = child.kill();
            let _ = child.wait();
            let mut msg = String::with_capacity(self.program.len() + 30);
            let _ = write!(
                msg,
                "{} timed out after {}s",
                self.program, request.timeout_seconds
            );
            return Err(HarnessError::Agent(msg));
        }

        let output = child.wait_with_output();
        match output {
            Ok(output) => {
                let exit_status = output.status.code();
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let mut msg = String::with_capacity(
                        self.program.len() + stderr.len() + stdout.len() + 64,
                    );
                    let _ = write!(
                        msg,
                        "{} exited with {:?}\n--- stderr ---\n{}\n--- stdout ---\n{}",
                        self.program, exit_status, stderr, stdout
                    );
                    return Err(HarnessError::Agent(msg));
                }
                Ok(AgentResponse {
                    stdout: String::from_utf8(output.stdout)?,
                    stderr: String::from_utf8(output.stderr)?,
                    exit_status,
                    duration: started.elapsed(),
                    artifact_paths: Vec::new(),
                })
            }
            Err(e) => {
                let mut msg = String::with_capacity(self.program.len() + 40);
                let _ = write!(msg, "failed to collect {} output: {e}", self.program);
                Err(HarnessError::Agent(msg))
            }
        }
    }
}
