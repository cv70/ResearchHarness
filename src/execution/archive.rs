use std::{
    fs,
    path::{Path, PathBuf},
};

use chrono::Utc;

use crate::core::{Experiment, ExperimentArchive, ExperimentStatus, Result};

#[derive(Debug, Clone)]
pub struct ArchiveStore {
    root: PathBuf,
}

impl ArchiveStore {
    pub fn new(workspace_root: impl AsRef<Path>, run_tag: &str) -> Self {
        Self {
            root: workspace_root
                .as_ref()
                .join(".research-harness")
                .join("runs")
                .join(run_tag),
        }
    }

    pub fn init_run_dirs(&self) -> Result<()> {
        fs::create_dir_all(self.root.join("prompts"))?;
        fs::create_dir_all(self.root.join("experiments"))?;
        Ok(())
    }

    pub fn experiment_archive(&self, experiment_id: &str) -> Result<ExperimentArchive> {
        let dir = self.root.join("experiments").join(experiment_id);
        fs::create_dir_all(&dir)?;
        Ok(ExperimentArchive {
            manifest_path: dir.join("manifest.toml"),
            plan_path: dir.join("plan.md"),
            diff_path: dir.join("diff.patch"),
            run_log_path: dir.join("run.log"),
            log_excerpt_path: dir.join("log_excerpt.md"),
            analysis_path: dir.join("analysis.md"),
            reflection_path: dir.join("reflection.md"),
        })
    }

    #[must_use]
    pub fn state_path(&self) -> PathBuf {
        self.root.join("state.toml")
    }

    pub fn write_manifest(&self, experiment: &Experiment) -> Result<()> {
        let manifest = toml::to_string_pretty(experiment)?;
        fs::write(experiment.archive_path.join("manifest.toml"), manifest)?;
        Ok(())
    }

    pub fn write_text(path: impl AsRef<Path>, content: impl AsRef<str>) -> Result<()> {
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content.as_ref())?;
        Ok(())
    }

    pub fn create_experiment(
        &self,
        run_tag: &str,
        experiment_index: u64,
        base_commit: String,
    ) -> Result<(Experiment, ExperimentArchive)> {
        let experiment_id = format!("exp-{experiment_index:05}");
        let archive = self.experiment_archive(&experiment_id)?;
        let experiment = Experiment {
            id: experiment_id,
            run_tag: run_tag.to_string(),
            base_commit,
            candidate_commit: None,
            status: ExperimentStatus::Planned,
            hypothesis: None,
            metric_snapshot: None,
            archive_path: archive
                .manifest_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf(),
            debug_attempts: 0,
        };
        Self::write_text(
            &archive.manifest_path,
            format!("# created {}\n", Utc::now().to_rfc3339()),
        )?;
        Ok((experiment, archive))
    }
}

pub fn write_log_excerpt(
    content: &str,
    destination: impl AsRef<Path>,
    max_lines: usize,
) -> Result<()> {
    let excerpt = if max_lines == 0 {
        String::new()
    } else {
        let mut ring: std::collections::VecDeque<&str> =
            std::collections::VecDeque::with_capacity(max_lines);
        for line in content.lines() {
            if ring.len() == max_lines {
                ring.pop_front();
            }
            ring.push_back(line);
        }
        ring.into_iter().collect::<Vec<_>>().join("\n")
    };
    ArchiveStore::write_text(destination, excerpt)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn creates_experiment_archive() {
        let dir = tempdir().unwrap();
        let store = ArchiveStore::new(dir.path(), "test");
        store.init_run_dirs().unwrap();
        let (_experiment, archive) = store
            .create_experiment("test", 1, "base".to_string())
            .unwrap();
        assert!(archive.manifest_path.exists());
        assert!(archive.plan_path.parent().unwrap().exists());
    }

    #[test]
    fn writes_tail_excerpt() {
        let dir = tempdir().unwrap();
        let destination = dir.path().join("excerpt.md");
        write_log_excerpt("a\nb\nc\nd\n", &destination, 2).unwrap();
        assert_eq!(fs::read_to_string(destination).unwrap(), "c\nd");
    }
}
