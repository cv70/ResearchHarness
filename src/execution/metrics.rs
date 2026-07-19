use std::path::Path;

use crate::{
    config::MetricConfig,
    core::{HarnessError, MetricSnapshot, Result},
    policy::is_improved,
};

pub fn parse_metric(
    config: &MetricConfig,
    log_content: &str,
    source_log: impl AsRef<Path>,
    previous_best: Option<f64>,
) -> Result<MetricSnapshot> {
    let regex = config.compiled_regex()?;
    let capture = regex
        .captures(log_content)
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
        source_log: source_log.as_ref().to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::core::MetricDirection;

    #[test]
    fn parses_metric_and_compares_lower() {
        let config = MetricConfig {
            name: "val_bpb".to_string(),
            regex: "^val_bpb:\\s+([0-9.]+)".to_string(),
            direction: MetricDirection::Lower,
        };
        let snapshot = parse_metric(
            &config,
            "val_bpb:          0.997900\n",
            Path::new("run.log"),
            Some(1.0),
        )
        .unwrap();
        assert_eq!(snapshot.value, 0.9979);
        assert!(snapshot.improved);
    }

    #[test]
    fn parses_metric_and_compares_higher() {
        let config = MetricConfig {
            name: "accuracy".to_string(),
            regex: "^accuracy:\\s+([0-9.]+)".to_string(),
            direction: MetricDirection::Higher,
        };
        let snapshot =
            parse_metric(&config, "accuracy: 0.95\n", Path::new("run.log"), Some(0.9)).unwrap();
        assert_eq!(snapshot.value, 0.95);
        assert!(snapshot.improved);
    }

    #[test]
    fn returns_error_when_metric_not_found() {
        let config = MetricConfig {
            name: "val_bpb".to_string(),
            regex: "^val_bpb:\\s+([0-9.]+)".to_string(),
            direction: MetricDirection::Lower,
        };
        let result = parse_metric(
            &config,
            "no relevant content here\n",
            Path::new("run.log"),
            None,
        );
        assert!(matches!(result, Err(HarnessError::MetricNotFound(_))));
    }

    #[test]
    fn first_metric_always_improves() {
        let config = MetricConfig {
            name: "score".to_string(),
            regex: "^score:\\s+([0-9.]+)".to_string(),
            direction: MetricDirection::Higher,
        };
        let snapshot = parse_metric(&config, "score: 42.5\n", Path::new("run.log"), None).unwrap();
        assert!(snapshot.improved);
    }
}
