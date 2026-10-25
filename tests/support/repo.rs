use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

pub struct TempRepo {
    dir: TempDir,
}

impl TempRepo {
    pub fn new() -> Self {
        let repo = Self {
            dir: TempDir::new().expect("temp dir"),
        };
        repo.git(&["init", "-b", "main"]);
        repo.git(&["config", "user.name", "Test"]);
        repo.git(&["config", "user.email", "test@example.com"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo
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
        let output = Command::new("git")
            .arg("-C")
            .arg(self.dir.path())
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
}
