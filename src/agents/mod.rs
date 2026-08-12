pub mod cli_runner;
pub mod mock;

use std::{borrow::Cow, path::PathBuf, sync::Arc, time::Duration};

use crate::core::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRole {
    Coordinator,
    Research,
    Planning,
    Coding,
    Review,
    Analyst,
    Memory,
}

#[derive(Debug, Clone)]
pub struct AgentRequest {
    pub role: AgentRole,
    pub working_directory: PathBuf,
    pub system_prompt: Cow<'static, str>,
    pub task_prompt: String,
    pub allowed_paths: Arc<[PathBuf]>,
    pub context_files: Vec<PathBuf>,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct AgentResponse {
    pub stdout: String,
    pub stderr: String,
    pub exit_status: Option<i32>,
    pub duration: Duration,
    pub artifact_paths: Vec<PathBuf>,
}

pub trait AgentRunner {
    fn run(&self, request: &AgentRequest) -> Result<AgentResponse>;
}
