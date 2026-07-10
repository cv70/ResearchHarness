use std::time::{Duration, Instant};

use crate::{
    agents::{AgentRequest, AgentResponse, AgentRunner},
    core::Result,
};

#[derive(Debug, Default, Clone)]
pub struct MockAgentRunner;

impl AgentRunner for MockAgentRunner {
    fn run(&self, request: &AgentRequest) -> Result<AgentResponse> {
        let started = Instant::now();
        Ok(AgentResponse {
            stdout: format!("mock response for {:?}", request.role),
            stderr: String::new(),
            exit_status: Some(0),
            duration: started.elapsed().max(Duration::from_millis(1)),
            artifact_paths: Vec::new(),
        })
    }
}
