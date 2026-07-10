pub mod cli_runner;
pub mod mock;

use std::{path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

use crate::core::Result;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AgentRole {
    Coordinator,
    Research,
    Planning,
    Coding,
    Review,
    Debug,
    Analyst,
    Memory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRequest {
    pub role: AgentRole,
    pub working_directory: PathBuf,
    pub system_prompt: String,
    pub task_prompt: String,
    pub allowed_paths: Vec<PathBuf>,
    pub context_files: Vec<PathBuf>,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
