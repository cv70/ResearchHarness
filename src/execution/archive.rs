use std::{
    fs,
    io::{BufReader, Read, Seek, SeekFrom},
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

    pub fn write_manifest(
        &self,
        manifest_path: impl AsRef<Path>,
        experiment: &Experiment,
    ) -> Result<()> {
        let manifest = toml::to_string_pretty(experiment)?;
        fs::write(manifest_path, manifest)?;
        Ok(())
    }

    pub fn write_text(path: impl AsRef<Path>, content: impl AsRef<str>) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
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
        };
        Self::write_text(
            &archive.manifest_path,
            format!("# created {}\n", Utc::now().to_rfc3339()),
        )?;
        Ok((experiment, archive))
    }
}

pub fn build_log_excerpt(content: &str, max_lines: usize) -> String {
    if max_lines == 0 {
        return String::new();
    }
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].join("\n")
}

pub fn read_log_excerpt(path: impl AsRef<Path>, max_lines: usize) -> Result<String> {
    if max_lines == 0 {
        return Ok(String::new());
    }
    let file = fs::File::open(path)?;
    let file_size = file.metadata()?.len() as usize;
    if file_size == 0 {
        return Ok(String::new());
    }

    let mut reader = BufReader::new(file);
    let chunk_size = 8192;
    let mut pos = file_size;
    let mut buffer = String::new();
    let mut newline_count = 0;

    while pos > 0 && newline_count < max_lines {
        let read_size = chunk_size.min(pos);
        pos -= read_size;
        reader.seek(SeekFrom::Start(pos as u64))?;
        let mut chunk = String::with_capacity(read_size);
        reader
            .by_ref()
            .take(read_size as u64)
            .read_to_string(&mut chunk)?;
        buffer.insert_str(0, &chunk);

        newline_count = buffer.bytes().filter(|&b| b == b'\n').count();

        if newline_count >= max_lines && pos > 0 {
            let offset = newline_count.saturating_sub(max_lines);
            let idx = buffer
                .as_bytes()
                .iter()
                .enumerate()
                .filter(|(_, b)| **b == b'\n')
                .nth(offset)
                .map(|(i, _)| i + 1);
            if let Some(idx) = idx {
                buffer.drain(..idx);
            }
            break;
        }
    }

    if buffer.ends_with('\n') {
        buffer.pop();
    }

    Ok(buffer.lines().collect::<Vec<_>>().join("\n"))
}

pub fn write_log_excerpt(
    content: &str,
    destination: impl AsRef<Path>,
    max_lines: usize,
) -> Result<()> {
    let excerpt = build_log_excerpt(content, max_lines);
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

    #[test]
    fn reads_tail_excerpt_from_large_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("big.log");
        let content: String = (0..1000)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, &content).unwrap();
        let excerpt = read_log_excerpt(&path, 5).unwrap();
        assert_eq!(excerpt, "line 995\nline 996\nline 997\nline 998\nline 999");
    }
}
