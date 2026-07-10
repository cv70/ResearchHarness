use std::path::{Path, PathBuf};

use crate::core::{HarnessError, MetricDirection, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathPolicy {
    modifiable: Vec<PathBuf>,
    readonly: Vec<PathBuf>,
}

impl PathPolicy {
    pub fn new(modifiable: Vec<String>, readonly: Vec<String>) -> Self {
        Self {
            modifiable: modifiable.into_iter().map(PathBuf::from).collect(),
            readonly: readonly.into_iter().map(PathBuf::from).collect(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.modifiable.is_empty() {
            return Err(HarnessError::InvalidConfig(
                "at least one modifiable path is required".to_string(),
            ));
        }
        for path in self.modifiable.iter().chain(self.readonly.iter()) {
            if path.is_absolute() {
                return Err(HarnessError::InvalidConfig(format!(
                    "path policy entries must be relative: {}",
                    path.display()
                )));
            }
            if path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(HarnessError::InvalidConfig(format!(
                    "path policy entries cannot contain ..: {}",
                    path.display()
                )));
            }
        }
        Ok(())
    }

    pub fn check_changed_paths<I, P>(&self, paths: I) -> Result<()>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        for path in paths {
            let path = path.as_ref();
            if self.matches_any(path, &self.readonly) {
                return Err(HarnessError::PathPolicy(format!(
                    "readonly path changed: {}",
                    path.display()
                )));
            }
            if !self.matches_any(path, &self.modifiable) {
                return Err(HarnessError::PathPolicy(format!(
                    "path is outside modifiable set: {}",
                    path.display()
                )));
            }
        }
        Ok(())
    }

    fn matches_any(&self, path: &Path, patterns: &[PathBuf]) -> bool {
        patterns.iter().any(|pattern| {
            path == pattern || path.starts_with(pattern) || pattern.to_string_lossy() == "*"
        })
    }
}

pub fn is_improved(current: f64, previous_best: Option<f64>, direction: MetricDirection) -> bool {
    match previous_best {
        None => true,
        Some(best) => match direction {
            MetricDirection::Lower => current < best,
            MetricDirection::Higher => current > best,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn accepts_modifiable_path() {
        let policy = PathPolicy::new(vec!["src".to_string()], vec!["README.md".to_string()]);
        policy
            .check_changed_paths([PathBuf::from("src/main.rs")])
            .unwrap();
    }

    #[test]
    fn rejects_readonly_path() {
        let policy = PathPolicy::new(vec!["src".to_string()], vec!["src/locked.rs".to_string()]);
        assert!(
            policy
                .check_changed_paths([PathBuf::from("src/locked.rs")])
                .is_err()
        );
    }

    #[test]
    fn compares_metric_direction() {
        assert!(is_improved(0.9, Some(1.0), MetricDirection::Lower));
        assert!(!is_improved(1.1, Some(1.0), MetricDirection::Lower));
        assert!(is_improved(1.1, Some(1.0), MetricDirection::Higher));
    }
}
