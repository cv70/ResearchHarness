use std::path::{Path, PathBuf};

use crate::core::{HarnessError, MetricDirection, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathPolicy {
    modifiable: Vec<PathPattern>,
    readonly: Vec<PathPattern>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PathPattern {
    Exact(PathBuf),
    Prefix(PathBuf),
    Wildcard,
    Extension(String),
}

impl PathPattern {
    fn parse(pattern: &str) -> Self {
        let path = PathBuf::from(pattern);
        if pattern == "*" {
            PathPattern::Wildcard
        } else if let Some(ext) = pattern.strip_prefix("*.") {
            PathPattern::Extension(ext.to_string())
        } else if pattern.ends_with('/') {
            PathPattern::Prefix(path)
        } else {
            PathPattern::Exact(path)
        }
    }

    fn matches(&self, path: &Path) -> bool {
        match self {
            PathPattern::Wildcard => true,
            PathPattern::Exact(pattern) => path == pattern || path.starts_with(pattern),
            PathPattern::Prefix(pattern) => path.starts_with(pattern),
            PathPattern::Extension(ext) => path
                .extension()
                .map(|e| e.to_string_lossy() == *ext)
                .unwrap_or(false),
        }
    }
}

impl PathPolicy {
    pub fn new(modifiable: Vec<String>, readonly: Vec<String>) -> Self {
        Self {
            modifiable: modifiable
                .into_iter()
                .map(|p| PathPattern::parse(&p))
                .collect(),
            readonly: readonly
                .into_iter()
                .map(|p| PathPattern::parse(&p))
                .collect(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.modifiable.is_empty() {
            return Err(HarnessError::InvalidConfig(
                "at least one modifiable path is required".to_string(),
            ));
        }
        for pattern in self.modifiable.iter().chain(self.readonly.iter()) {
            if let PathPattern::Exact(path) | PathPattern::Prefix(path) = pattern {
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

    fn matches_any(&self, path: &Path, patterns: &[PathPattern]) -> bool {
        patterns.iter().any(|pattern| pattern.matches(path))
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

    #[test]
    fn accepts_extension_pattern() {
        let policy = PathPolicy::new(vec!["*.rs".to_string()], vec![]);
        policy
            .check_changed_paths([PathBuf::from("src/main.rs"), PathBuf::from("lib.rs")])
            .unwrap();
    }

    #[test]
    fn rejects_non_matching_extension() {
        let policy = PathPolicy::new(vec!["*.rs".to_string()], vec![]);
        assert!(
            policy
                .check_changed_paths([PathBuf::from("src/main.py")])
                .is_err()
        );
    }

    #[test]
    fn accepts_prefix_pattern() {
        let policy = PathPolicy::new(vec!["src/".to_string()], vec![]);
        policy
            .check_changed_paths([
                PathBuf::from("src/main.rs"),
                PathBuf::from("src/utils/helper.rs"),
            ])
            .unwrap();
    }

    #[test]
    fn rejects_outside_prefix() {
        let policy = PathPolicy::new(vec!["src/".to_string()], vec![]);
        assert!(
            policy
                .check_changed_paths([PathBuf::from("tests/test.rs")])
                .is_err()
        );
    }

    #[test]
    fn accepts_wildcard_pattern() {
        let policy = PathPolicy::new(vec!["*".to_string()], vec!["README.md".to_string()]);
        policy
            .check_changed_paths([
                PathBuf::from("src/main.rs"),
                PathBuf::from("data/config.yaml"),
            ])
            .unwrap();
    }

    #[test]
    fn rejects_exact_readonly_with_prefix_modifiable() {
        let policy = PathPolicy::new(vec!["src/".to_string()], vec!["src/config.rs".to_string()]);
        assert!(
            policy
                .check_changed_paths([PathBuf::from("src/config.rs")])
                .is_err()
        );
    }

    #[test]
    fn exact_match_takes_precedence_over_prefix() {
        let policy = PathPolicy::new(vec!["src/".to_string()], vec!["src/main.rs".to_string()]);
        assert!(
            policy
                .check_changed_paths([PathBuf::from("src/main.rs")])
                .is_err()
        );
        policy
            .check_changed_paths([PathBuf::from("src/utils.rs")])
            .unwrap();
    }
}
