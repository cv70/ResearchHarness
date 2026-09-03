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
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
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
        Ok(self
            .porcelain_paths()?
            .into_iter()
            .map(PathBuf::from)
            .collect())
    }

    pub fn user_changed_files(&self) -> Result<Vec<PathBuf>> {
        Ok(self
            .porcelain_paths()?
            .into_iter()
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
        let out = self.git(["commit", "-m", message])?;
        extract_commit_sha(&out)
    }

    pub fn commit_paths<I, P>(&self, paths: I, message: &str) -> Result<String>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        self.add_paths(paths)?;
        let out = self.git(["commit", "-m", message])?;
        extract_commit_sha(&out)
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
        Ok(!self.porcelain_paths()?.is_empty())
    }

    pub fn has_user_changes(&self) -> Result<bool> {
        Ok(self
            .porcelain_paths()?
            .iter()
            .any(|path| !path.starts_with(".research-harness")))
    }

    fn porcelain_paths(&self) -> Result<Vec<String>> {
        Ok(self
            .git(["status", "--porcelain"])?
            .lines()
            .filter_map(|line| line.get(3..).map(str::to_string))
            .collect())
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

fn extract_commit_sha(commit_output: &str) -> Result<String> {
    // `git commit` stdout starts with something like:
    //   "[branch abc1234] commit message"
    // or on detached HEAD: "[detached HEAD abc1234] ..."
    let line = commit_output.lines().next().unwrap_or("");
    let bracketed = line
        .strip_prefix('[')
        .and_then(|l| l.split_once(']'))
        .map(|(inner, _)| inner)
        .unwrap_or(line);
    let sha = bracketed
        .rsplit_once(' ')
        .map(|(_, s)| s)
        .unwrap_or(bracketed);
    if sha.is_empty() {
        Err(HarnessError::CommandFailed {
            program: "git".to_string(),
            args: vec!["rev-parse".to_string(), "HEAD".to_string()],
            stderr: "commit produced no sha in output".to_string(),
        })
    } else {
        Ok(sha.to_string())
    }
}
