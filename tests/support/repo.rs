use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

pub struct TempRepo {
    dir: TempDir,
    remote: Option<TempDir>,
}

impl TempRepo {
    pub fn new() -> Self {
        let repo = Self {
            dir: TempDir::new().expect("temp dir"),
            remote: None,
        };
        repo.git(&["init", "-b", "main"]);
        repo.configure_identity();
        repo
    }

    pub fn with_remote() -> Self {
        let remote = TempDir::new().expect("temp dir");
        run_git(remote.path(), &["init", "--bare", "-b", "main"]);
        let mut repo = Self::new();
        repo.git(&["remote", "add", "origin", &remote.path().to_string_lossy()]);
        repo.write("README.md", "seed\n");
        repo.commit_all("seed");
        repo.git(&["push", "-u", "origin", "main"]);
        repo.remote = Some(remote);
        repo
    }

    pub fn clone_remote(&self) -> Self {
        let remote = self.remote.as_ref().expect("repo has a remote");
        let clone = Self {
            dir: TempDir::new().expect("temp dir"),
            remote: None,
        };
        run_git(
            clone.path(),
            &["clone", &remote.path().to_string_lossy(), "."],
        );
        clone.configure_identity();
        clone
    }

    pub fn remote_log(&self) -> String {
        let remote = self.remote.as_ref().expect("repo has a remote");
        run_git(remote.path(), &["log", "--format=%s", "main"])
    }

    pub fn push(&self) {
        self.git(&["push"]);
    }

    fn configure_identity(&self) {
        self.git(&["config", "user.name", "Test"]);
        self.git(&["config", "user.email", "test@example.com"]);
        self.git(&["config", "commit.gpgsign", "false"]);
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn write(&self, relative: &str, content: &str) {
        let path = self.dir.path().join(relative);
        fs::create_dir_all(path.parent().expect("parent")).expect("create dirs");
        fs::write(path, content).expect("write file");
    }

    pub fn commit_all(&self, message: &str) -> String {
        self.git(&["add", "--all"]);
        self.git(&["commit", "-m", message]);
        self.git(&["rev-parse", "HEAD"]).trim().to_owned()
    }

    fn git(&self, args: &[&str]) -> String {
        run_git(self.dir.path(), args)
    }
}

fn run_git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf8")
}
