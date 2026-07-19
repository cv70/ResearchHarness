use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    core::{HarnessError, MetricDirection, Result},
    policy::PathPolicy,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub project: ProjectConfig,
    pub workspace: WorkspaceConfig,
    pub experiment: ExperimentConfig,
    pub metric: MetricConfig,
    pub agent: AgentConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectConfig {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceConfig {
    pub modifiable: Vec<String>,
    #[serde(default)]
    pub readonly: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExperimentConfig {
    pub command: String,
    #[serde(default = "default_log_file")]
    pub log_file: String,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(default = "default_archive_logs")]
    pub archive_logs: bool,
    #[serde(default = "default_max_log_excerpt_lines")]
    pub max_log_excerpt_lines: usize,
    #[serde(default = "default_max_debug_attempts")]
    pub max_debug_attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricConfig {
    pub name: String,
    pub regex: String,
    pub direction: MetricDirection,
}

impl MetricConfig {
    pub fn compiled_regex(&self) -> Result<regex::Regex> {
        regex::Regex::new(&self.regex).map_err(Into::into)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentConfig {
    #[serde(default = "default_agent_backend")]
    pub backend: String,
}

impl Config {
    pub fn load(root: impl AsRef<Path>) -> Result<Self> {
        let path = root.as_ref().join("research.toml");
        let raw = fs::read_to_string(path)?;
        let config: Self = toml::from_str(&raw)?;
        config.validate()?;
        Ok(config)
    }

    pub fn write_default(root: impl AsRef<Path>) -> Result<()> {
        let path = root.as_ref().join("research.toml");
        if path.exists() {
            return Err(HarnessError::InvalidConfig(
                "research.toml already exists".to_string(),
            ));
        }
        fs::write(path, Self::default_toml())?;
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        require_non_empty(&self.project.name, "project.name")?;
        if self.workspace.modifiable.is_empty() {
            return Err(HarnessError::InvalidConfig(
                "workspace.modifiable cannot be empty".to_string(),
            ));
        }
        require_non_empty(&self.experiment.command, "experiment.command")?;
        require_non_empty(&self.metric.name, "metric.name")?;
        require_non_empty(&self.metric.regex, "metric.regex")?;
        self.metric
            .compiled_regex()
            .map_err(|e| HarnessError::InvalidConfig(format!("invalid metric regex: {e}")))?;
        let policy = PathPolicy::new(
            self.workspace.modifiable.clone(),
            self.workspace.readonly.clone(),
        );
        policy.validate()?;
        Ok(())
    }

    #[must_use]
    pub fn default_toml() -> &'static str {
        r#"[project]
name = "autoresearch"

[workspace]
modifiable = ["train.py"]
readonly = ["prepare.py", "research.toml"]

[experiment]
command = "uv run train.py"
log_file = "run.log"
timeout_seconds = 600
archive_logs = true
max_log_excerpt_lines = 200
max_debug_attempts = 1

[metric]
name = "val_bpb"
regex = "^val_bpb:\\s+([0-9.]+)"
direction = "lower"

[agent]
backend = "mock"
"#
    }
}

fn default_log_file() -> String {
    "run.log".to_string()
}

fn require_non_empty(value: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(HarnessError::InvalidConfig(format!(
            "{field} cannot be empty"
        )));
    }
    Ok(())
}

fn default_timeout_seconds() -> u64 {
    600
}

fn default_archive_logs() -> bool {
    true
}

fn default_max_log_excerpt_lines() -> usize {
    200
}

fn default_max_debug_attempts() -> u32 {
    1
}

fn default_agent_backend() -> String {
    "mock".to_string()
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn loads_default_config() {
        let dir = tempdir().unwrap();
        Config::write_default(dir.path()).unwrap();
        let config = Config::load(dir.path()).unwrap();
        assert_eq!(config.project.name, "autoresearch");
        assert_eq!(config.experiment.max_debug_attempts, 1);
        assert_eq!(config.agent.backend, "mock");
    }

    #[test]
    fn rejects_empty_modifiable_paths() {
        let raw = r#"
[project]
name = "x"

[workspace]
modifiable = []

[experiment]
command = "echo ok"

[metric]
name = "score"
regex = "score: ([0-9.]+)"
direction = "higher"

[agent]
backend = "mock"
"#;
        let config: Config = toml::from_str(raw).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_invalid_regex() {
        let raw = r#"
[project]
name = "x"

[workspace]
modifiable = ["train.py"]

[experiment]
command = "echo ok"

[metric]
name = "score"
regex = "score: ([0-9.]+"
direction = "higher"

[agent]
backend = "mock"
"#;
        let config: Config = toml::from_str(raw).unwrap();
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("invalid metric regex"));
    }
}
