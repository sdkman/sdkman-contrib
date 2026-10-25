use std::path::Path;

use crate::git::Git;
use crate::request_path::{APPLIED_DIR, REQUESTS_DIR};

pub fn added_request(repo_root: &Path, base: &str) -> Result<String, String> {
    let diff = Git::new(repo_root)
        .run_as(
            &format!("diff against {base}"),
            &["diff", "--name-status", "--no-renames", base, "HEAD"],
        )
        .map_err(|error| error.to_string())?;
    let changes: Vec<(&str, &str)> = diff
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .collect();

    if let Some((_, path)) = changes.iter().find(|(_, path)| is_below(path, APPLIED_DIR)) {
        return Err(format!(
            "{path} is under {APPLIED_DIR}/; only the apply bot writes receipts"
        ));
    }
    match changes.as_slice() {
        [("A", path)] if is_below(path, REQUESTS_DIR) => Ok((*path).to_owned()),
        _ => Err("a request PR adds exactly one file under requests/".to_owned()),
    }
}

fn is_below(path: &str, dir: &str) -> bool {
    path.strip_prefix(dir)
        .is_some_and(|rest| rest.starts_with('/'))
}
