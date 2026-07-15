use std::{
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

        let prompt = format!("{}\n\n{}", request.system_prompt, request.task_prompt);
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(prompt.as_bytes())?;
        }

        let timeout = Duration::from_secs(request.timeout_seconds);
        match child.wait_timeout(timeout)? {
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(HarnessError::Agent(format!(
                    "{} timed out after {}s",
                    self.program, request.timeout_seconds
                )));
            }
            Some(_) => {}
        }

        let output = child.wait_with_output()?;
        let exit_status = output.status.code();
        if !output.status.success() {
            return Err(HarnessError::Agent(format!(
                "{} exited with {:?}: {}",
                self.program,
                exit_status,
                String::from_utf8(output.stderr)?
            )));
        }
        Ok(AgentResponse {
            stdout: String::from_utf8(output.stdout)?,
            stderr: String::from_utf8(output.stderr)?,
            exit_status,
            duration: started.elapsed(),
            artifact_paths: Vec::new(),
        })
    }
}
