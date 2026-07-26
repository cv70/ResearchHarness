use std::{
    cell::Cell,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use crate::core::Result;

#[derive(Debug, Clone)]
pub struct MemoryStore {
    root: PathBuf,
    initialized: Cell<bool>,
}

#[derive(Debug, Clone)]
pub struct MemorySnapshot {
    pub business: String,
    pub experiments: String,
    pub decisions: String,
    pub playbook: String,
}

const DEFAULT_CONTENTS: &[(&str, &str)] = &[
    ("business.md", "# 业务说明\n\n"),
    ("experiments.md", "# 实验记录\n\n"),
    ("decisions.md", "# 决策记忆\n\n"),
    ("playbook.md", "# 研究手册\n\n"),
];

impl MemoryStore {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        Self {
            root: workspace_root
                .as_ref()
                .join(".research-harness")
                .join("memory"),
            initialized: Cell::new(false),
        }
    }

    pub fn init(&self) -> Result<()> {
        if self.initialized.get() {
            return Ok(());
        }
        fs::create_dir_all(&self.root)?;
        for &(name, content) in DEFAULT_CONTENTS {
            self.ensure_file(name, content)?;
        }
        self.initialized.set(true);
        Ok(())
    }

    pub fn load(&self) -> Result<MemorySnapshot> {
        self.init()?;
        Ok(MemorySnapshot {
            business: self.read("business.md")?,
            experiments: self.read("experiments.md")?,
            decisions: self.read("decisions.md")?,
            playbook: self.read("playbook.md")?,
        })
    }

    pub fn append_business(&self, text: &str) -> Result<()> {
        self.init()?;
        self.append("business.md", text)
    }

    pub fn append_experiment(&self, text: &str) -> Result<()> {
        self.init()?;
        self.append("experiments.md", text)
    }

    pub fn append_decision(&self, text: &str) -> Result<()> {
        self.init()?;
        self.append("decisions.md", text)
    }

    pub fn append_playbook(&self, text: &str) -> Result<()> {
        self.init()?;
        self.append("playbook.md", text)
    }

    fn ensure_file(&self, name: &str, content: &str) -> Result<()> {
        let path = self.root.join(name);
        if !path.exists() {
            fs::write(path, content)?;
        }
        Ok(())
    }

    fn read(&self, name: &str) -> Result<String> {
        Ok(fs::read_to_string(self.root.join(name))?)
    }

    fn append(&self, name: &str, text: &str) -> Result<()> {
        let path = self.root.join(name);
        let mut file = OpenOptions::new().append(true).open(&path)?;
        if !text.starts_with('\n') {
            writeln!(file)?;
        }
        writeln!(file, "{text}")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn initializes_and_appends_memory() {
        let dir = tempdir().unwrap();
        let store = MemoryStore::new(dir.path());
        store.init().unwrap();
        store.append_business("目标：降低 val_bpb").unwrap();
        let snapshot = store.load().unwrap();
        assert!(snapshot.business.contains("目标：降低 val_bpb"));
        assert!(snapshot.experiments.contains("# 实验记录"));
        assert!(snapshot.decisions.contains("# 决策记忆"));
        assert!(snapshot.playbook.contains("# 研究手册"));
    }
}
