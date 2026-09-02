use std::{fmt::Write as _, fs, fs::OpenOptions, io::Write, path::Path};

use serde::{Deserialize, Serialize};

use crate::core::{HarnessError, MetricDirection, Result};

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
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(default = "default_max_log_excerpt_lines")]
    pub max_log_excerpt_lines: usize,
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
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|err| {
                if err.kind() == std::io::ErrorKind::AlreadyExists {
                    HarnessError::InvalidConfig("research.toml already exists".to_string())
                } else {
                    HarnessError::Io(err)
                }
            })?;
        file.write_all(Self::default_toml().as_bytes())?;
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
        validate_path_patterns(&self.workspace.modifiable)?;
        validate_path_patterns(&self.workspace.readonly)?;
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
timeout_seconds = 600
max_log_excerpt_lines = 200

[metric]
name = "val_bpb"
regex = "^val_bpb:\\s+([0-9.]+)"
direction = "lower"

[agent]
backend = "mock"
"#
    }
}

fn require_non_empty(value: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() {
        let mut msg = String::with_capacity(field.len() + 16);
        write!(msg, "{field} cannot be empty").unwrap();
        return Err(HarnessError::InvalidConfig(msg));
    }
    Ok(())
}

fn default_timeout_seconds() -> u64 {
    600
}

fn default_max_log_excerpt_lines() -> usize {
    200
}

fn default_agent_backend() -> String {
    "mock".to_string()
}

fn validate_path_patterns(patterns: &[String]) -> Result<()> {
    for pattern in patterns {
        if pattern == "*" || pattern.starts_with("*.") {
            continue;
        }
        let path = Path::new(pattern);
        if path.is_absolute() {
            let mut msg = String::with_capacity(32 + pattern.len());
            write!(msg, "path policy entries must be relative: {pattern}").unwrap();
            return Err(HarnessError::InvalidConfig(msg));
        }
        if path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            let mut msg = String::with_capacity(36 + pattern.len());
            write!(msg, "path policy entries cannot contain ..: {pattern}").unwrap();
            return Err(HarnessError::InvalidConfig(msg));
        }
    }
    Ok(())
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
        assert_eq!(config.experiment.timeout_seconds, 600);
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
