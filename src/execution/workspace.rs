use std::{
    path::{Path, PathBuf},
    process::Command,
};

use crate::core::{HarnessError, Result};

#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn ensure_git_repo(&self) -> Result<()> {
        self.git(["rev-parse", "--show-toplevel"])?;
        Ok(())
    }

    pub fn current_branch(&self) -> Result<String> {
        self.git(["rev-parse", "--abbrev-ref", "HEAD"])
            .map(|s| s.trim().to_string())
    }

    pub fn head_commit(&self) -> Result<String> {
        self.git(["rev-parse", "HEAD"])
            .map(|s| s.trim().to_string())
    }

    pub fn short_head(&self) -> Result<String> {
        self.git(["rev-parse", "--short", "HEAD"])
            .map(|s| s.trim().to_string())
    }

    pub fn changed_files(&self) -> Result<Vec<PathBuf>> {
        let out = self.git(["status", "--porcelain"])?;
        Ok(out
            .lines()
            .filter_map(|line| line.get(3..))
            .map(PathBuf::from)
            .collect())
    }

    pub fn user_changed_files(&self) -> Result<Vec<PathBuf>> {
        Ok(self
            .changed_files()?
            .into_iter()
            .filter(|path| !path.starts_with(".research-harness"))
            .collect())
    }

    pub fn diff(&self) -> Result<String> {
        self.git(["diff"])
    }

    pub fn add_all(&self) -> Result<()> {
        self.git(["add", "."])?;
        Ok(())
    }

    pub fn add_paths<I, P>(&self, paths: I) -> Result<()>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut args = vec!["add".to_string(), "--".to_string()];
        args.extend(
            paths
                .into_iter()
                .map(|path| path.as_ref().to_string_lossy().to_string()),
        );
        if args.len() == 2 {
            return Ok(());
        }
        self.git(args)?;
        Ok(())
    }

    pub fn commit(&self, message: &str) -> Result<String> {
        self.add_all()?;
        self.git(["commit", "-m", message])?;
        self.head_commit()
    }

    pub fn commit_paths<I, P>(&self, paths: I, message: &str) -> Result<String>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        self.add_paths(paths)?;
        self.git(["commit", "-m", message])?;
        self.head_commit()
    }

    pub fn reset_hard(&self, commit: &str) -> Result<()> {
        self.git(["reset", "--hard", commit])?;
        Ok(())
    }

    pub fn clean_user_untracked(&self) -> Result<()> {
        self.git([
            "clean",
            "-f",
            "-d",
            "--exclude=.research-harness/",
        ])?;
        Ok(())
    }

    pub fn checkout_new_branch(&self, branch: &str) -> Result<()> {
        self.git(["checkout", "-b", branch])?;
        Ok(())
    }

    pub fn is_dirty(&self) -> Result<bool> {
        Ok(!self.git(["status", "--porcelain"])?.trim().is_empty())
    }

    pub fn has_user_changes(&self) -> Result<bool> {
        Ok(!self.user_changed_files()?.is_empty())
    }

    fn git<I, S>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let args_vec: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
        let output = Command::new("git")
            .args(&args_vec)
            .current_dir(&self.root)
            .output()?;
        if !output.status.success() {
            return Err(HarnessError::CommandFailed {
                program: "git".to_string(),
                args: args_vec,
                stderr: String::from_utf8(output.stderr)?,
            });
        }
        String::from_utf8(output.stdout).map_err(Into::into)
    }
}
