use std::path::Path;

use chrono::{DateTime, Utc};

use crate::receipt::latest_applied;
use crate::request_path::{RequestPath, check_timestamp};

pub fn admit_timestamp(
    repo_root: &Path,
    path: &RequestPath,
    now: DateTime<Utc>,
) -> Result<(), String> {
    let latest = latest_applied(repo_root, path.candidate())
        .map_err(|error| format!("cannot read the receipts of {}: {error}", path.candidate()))?;
    check_timestamp(&path.timestamp(), now, latest.as_ref())
}
