use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
pub struct GitError(String);

impl fmt::Display for GitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for GitError {}

pub struct Git {
    root: PathBuf,
}

impl Git {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
        }
    }

    pub fn added_in(&self, path: &str) -> Result<String, GitError> {
        let output = self.run(&[
            "log",
            "--first-parent",
            "--diff-filter=A",
            "--format=%h",
            "-n",
            "1",
            "--",
            path,
        ])?;
        let commit = output.trim();
        if commit.is_empty() {
            return Err(GitError(format!("no commit adds {path}")));
        }
        Ok(commit.to_owned())
    }

    pub fn commit_and_push(&self, path: &str, message: &str) -> Result<(), GitError> {
        self.run(&["add", "--", path])?;
        self.run(&["commit", "-m", message])?;
        self.run(&["pull", "--rebase"])?;
        self.run(&["push"])?;
        Ok(())
    }

    pub fn run(&self, args: &[&str]) -> Result<String, GitError> {
        self.run_as(args[0], args)
    }

    pub fn run_as(&self, label: &str, args: &[&str]) -> Result<String, GitError> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(args)
            .output()
            .map_err(|error| GitError(format!("cannot run git: {error}")))?;
        if !output.status.success() {
            return Err(GitError(format!(
                "git {label} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}
