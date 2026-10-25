use std::path::Path;
use std::process::Command;

const REQUESTS_DIR: &str = "requests/";
const RECEIPTS_DIR: &str = ".applied/";

pub fn added_request(repo_root: &Path, base: &str) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["diff", "--name-status", "--no-renames", base, "HEAD"])
        .output()
        .map_err(|error| format!("cannot run git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git diff against {base} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let diff = String::from_utf8_lossy(&output.stdout);
    let changes: Vec<(&str, &str)> = diff
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .collect();

    if let Some((_, path)) = changes
        .iter()
        .find(|(_, path)| path.starts_with(RECEIPTS_DIR))
    {
        return Err(format!(
            "{path} is under .applied/; only the apply bot writes receipts"
        ));
    }
    match changes.as_slice() {
        [("A", path)] if path.starts_with(REQUESTS_DIR) => Ok((*path).to_owned()),
        _ => Err("a request PR adds exactly one file under requests/".to_owned()),
    }
}
