use std::{fs, path::Path};

use regex::Regex;

use crate::{
    config::MetricConfig,
    core::{HarnessError, MetricSnapshot, Result},
    policy::is_improved,
};

pub fn parse_metric(
    config: &MetricConfig,
    log_path: impl AsRef<Path>,
    previous_best: Option<f64>,
) -> Result<MetricSnapshot> {
    let raw = fs::read_to_string(&log_path)?;
    let regex = Regex::new(&config.regex)?;
    let capture = regex
        .captures(&raw)
        .and_then(|captures| captures.get(1))
        .ok_or_else(|| HarnessError::MetricNotFound(config.name.clone()))?;
    let value: f64 = capture.as_str().parse().map_err(|err| {
        HarnessError::Experiment(format!("metric {} is not a number: {err}", config.name))
    })?;
    let improved = is_improved(value, previous_best, config.direction);
    Ok(MetricSnapshot {
        name: config.name.clone(),
        value,
        previous_best,
        direction: config.direction,
        improved,
        source_log: log_path.as_ref().to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;
    use crate::core::MetricDirection;

    #[test]
    fn parses_metric_and_compares_lower() {
        let dir = tempdir().unwrap();
        let log = dir.path().join("run.log");
        fs::write(&log, "val_bpb:          0.997900\n").unwrap();
        let config = MetricConfig {
            name: "val_bpb".to_string(),
            regex: "^val_bpb:\\s+([0-9.]+)".to_string(),
            direction: MetricDirection::Lower,
        };
        let snapshot = parse_metric(&config, &log, Some(1.0)).unwrap();
        assert_eq!(snapshot.value, 0.9979);
        assert!(snapshot.improved);
    }
}
