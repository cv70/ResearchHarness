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

    #[must_use]
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

    pub fn changed_files(&self) -> Result<Vec<PathBuf>> {
        let out = self.git(["status", "--porcelain"])?;
        Ok(out
            .lines()
            .filter_map(|line| line.get(3..))
            .map(PathBuf::from)
            .collect())
    }

    pub fn user_changed_files(&self) -> Result<Vec<PathBuf>> {
        let out = self.git(["status", "--porcelain"])?;
        Ok(out
            .lines()
            .filter_map(|line| line.get(3..))
            .filter(|path| !path.starts_with(".research-harness"))
            .map(PathBuf::from)
            .collect())
    }

    pub fn diff(&self) -> Result<String> {
        self.git(["diff", "HEAD"])
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
        let mut iter = paths.into_iter().peekable();
        if iter.peek().is_none() {
            return Ok(());
        }
        let mut cmd = Command::new("git");
        cmd.arg("add").arg("--").current_dir(&self.root);
        let mut args: Vec<String> = vec!["add".to_string(), "--".to_string()];
        for path in iter {
            let p = path.as_ref();
            cmd.arg(p);
            args.push(p.display().to_string());
        }
        let output = cmd.output()?;
        if !output.status.success() {
            return Err(HarnessError::CommandFailed {
                program: "git".to_string(),
                args,
                stderr: String::from_utf8(output.stderr)?,
            });
        }
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
        self.git(["clean", "-f", "-d", "--exclude=.research-harness/"])?;
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
        let out = self.git(["status", "--porcelain"])?;
        Ok(out.lines().any(|line| {
            line.get(3..)
                .is_some_and(|path| !path.starts_with(".research-harness"))
        }))
    }

    fn git<const N: usize>(&self, args: [&str; N]) -> Result<String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .output()?;
        if !output.status.success() {
            return Err(HarnessError::CommandFailed {
                program: "git".to_string(),
                args: args.iter().map(|s| s.to_string()).collect(),
                stderr: String::from_utf8(output.stderr)?,
            });
        }
        String::from_utf8(output.stdout).map_err(Into::into)
    }
}
