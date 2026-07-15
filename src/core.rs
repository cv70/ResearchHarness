use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, HarnessError>;

#[derive(Debug, Error)]
pub enum HarnessError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml deserialize error: {0}")]
    TomlDe(#[from] toml::de::Error),
    #[error("toml serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error("regex error: {0}")]
    Regex(#[from] regex::Error),
    #[error("utf8 error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("command failed: {program} {args:?}: {stderr}")]
    CommandFailed {
        program: String,
        args: Vec<String>,
        stderr: String,
    },
    #[error("invalid config: {0}")]
    InvalidConfig(String),
    #[error("metric not found: {0}")]
    MetricNotFound(String),
    #[error("path policy violation: {0}")]
    PathPolicy(String),
    #[error("agent failed: {0}")]
    Agent(String),
    #[error("experiment failed: {0}")]
    Experiment(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Run {
    pub tag: String,
    pub branch: String,
    pub started_at: DateTime<Utc>,
    pub best_metric: Option<MetricSnapshot>,
    pub best_commit: Option<String>,
    pub experiment_count: u64,
    pub consecutive_crashes: u64,
    pub consecutive_regressions: u64,
}

impl Run {
    pub fn new(tag: impl Into<String>, branch: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            branch: branch.into(),
            started_at: Utc::now(),
            best_metric: None,
            best_commit: None,
            experiment_count: 0,
            consecutive_crashes: 0,
            consecutive_regressions: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum ExperimentStatus {
    Planned,
    Edited,
    Reviewed,
    Running,
    Kept,
    Discarded,
    Crashed,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Experiment {
    pub id: String,
    pub run_tag: String,
    pub base_commit: String,
    pub candidate_commit: Option<String>,
    pub status: ExperimentStatus,
    pub hypothesis: Option<String>,
    pub metric_snapshot: Option<MetricSnapshot>,
    pub archive_path: PathBuf,
    pub debug_attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExperimentArchive {
    pub manifest_path: PathBuf,
    pub plan_path: PathBuf,
    pub diff_path: PathBuf,
    pub run_log_path: PathBuf,
    pub log_excerpt_path: PathBuf,
    pub analysis_path: PathBuf,
    pub reflection_path: PathBuf,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[serde(rename_all = "lowercase")]
pub enum MetricDirection {
    Lower,
    Higher,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricSnapshot {
    pub name: String,
    pub value: f64,
    pub previous_best: Option<f64>,
    pub direction: MetricDirection,
    pub improved: bool,
    pub source_log: PathBuf,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum LearningLevel {
    SingleObservation,
    StableDecision,
    PlaybookRule,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Learning {
    pub summary: String,
    pub evidence: String,
    pub level: LearningLevel,
    pub source_experiment_ids: Vec<String>,
    pub recommended_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlaybookRule {
    pub rule: String,
    pub when_to_apply: String,
    pub why: String,
    pub evidence: String,
    pub priority: u32,
}
