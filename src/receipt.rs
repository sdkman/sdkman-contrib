use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::request_path::{APPLIED_DIR, NamingError, REQUESTS_DIR, RequestPath, Timestamp};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptRow {
    pub version: String,
    pub platform: String,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub md5: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha512: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StableTag {
    pub stable: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredMetadata {
    pub name: String,
    pub description: String,
    pub website: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub applied_at: String,
    pub commit: String,
    pub run: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<Vec<ReceiptRow>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<StableTag>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub register: Option<RegisteredMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voided: Option<String>,
}

#[derive(Debug)]
pub enum ReceiptError {
    Io(io::Error),
    Naming { path: String, error: NamingError },
    Yaml(serde_norway::Error),
}

impl fmt::Display for ReceiptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReceiptError::Io(error) => write!(f, "{error}"),
            ReceiptError::Naming { path, error } => write!(f, "{path}: {error}"),
            ReceiptError::Yaml(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ReceiptError {}

impl From<io::Error> for ReceiptError {
    fn from(error: io::Error) -> Self {
        ReceiptError::Io(error)
    }
}

pub fn latest_applied(
    repo_root: &Path,
    candidate: &str,
) -> Result<Option<Timestamp>, ReceiptError> {
    let dir = repo_root.join(APPLIED_DIR).join(candidate);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut latest = None;
    for entry in entries {
        let name = entry?.file_name();
        let timestamp = name
            .to_str()
            .and_then(|name| RequestPath::parse(&format!("{REQUESTS_DIR}/{candidate}/{name}")).ok())
            .map(|request| request.timestamp());
        latest = latest.max(timestamp);
    }
    Ok(latest)
}

pub fn pending_requests(repo_root: &Path) -> Result<Vec<RequestPath>, ReceiptError> {
    let mut relative_paths = Vec::new();
    collect_files(repo_root, Path::new(REQUESTS_DIR), &mut relative_paths)?;
    let mut pending = Vec::new();
    for relative in relative_paths {
        let path = relative.to_string_lossy().replace('\\', "/");
        let request = RequestPath::parse(&path).map_err(|error| ReceiptError::Naming {
            path: path.clone(),
            error,
        })?;
        if !repo_root.join(request.receipt_path()).exists() {
            pending.push(request);
        }
    }
    pending.sort();
    Ok(pending)
}

fn collect_files(
    repo_root: &Path,
    relative_dir: &Path,
    found: &mut Vec<PathBuf>,
) -> Result<(), ReceiptError> {
    let entries = match fs::read_dir(repo_root.join(relative_dir)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = entry?;
        let relative = relative_dir.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            collect_files(repo_root, &relative, found)?;
        } else {
            found.push(relative);
        }
    }
    Ok(())
}

pub fn write_receipt(
    repo_root: &Path,
    request: &RequestPath,
    receipt: &Receipt,
) -> Result<(), ReceiptError> {
    let yaml = serde_norway::to_string(receipt).map_err(ReceiptError::Yaml)?;
    write_new(repo_root, &request.receipt_path(), &yaml)?;
    Ok(())
}

pub fn write_new(repo_root: &Path, relative: &str, text: &str) -> io::Result<()> {
    let target = repo_root.join(relative);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?
        .write_all(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn touch(root: &Path, relative: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "x").unwrap();
    }

    fn request(path: &str) -> RequestPath {
        RequestPath::parse(path).unwrap()
    }

    fn receipt() -> Receipt {
        Receipt {
            applied_at: "2026-10-14T09:40:57Z".into(),
            commit: "9e07d13".into(),
            run: "https://github.com/sdkman/sdkman-contrib/actions/runs/123".into(),
            rows: None,
            tag: None,
            register: None,
            voided: None,
        }
    }

    #[test]
    fn missing_directories_hold_nothing() {
        let root = TempDir::new().unwrap();
        assert!(pending_requests(root.path()).unwrap().is_empty());
        assert_eq!(latest_applied(root.path(), "jpx").unwrap(), None);
    }

    #[test]
    fn pending_orders_across_candidates() {
        let root = TempDir::new().unwrap();
        touch(
            root.path(),
            "requests/zzz/20261008T140211Z-register-zzz.yaml",
        );
        touch(
            root.path(),
            "requests/aaa/20261009T081500Z-release-1.0.yaml",
        );
        touch(
            root.path(),
            "requests/bbb/20261009T081500Z-release-1.0.yaml",
        );
        let pending = pending_requests(root.path()).unwrap();
        let names: Vec<_> = pending
            .iter()
            .map(|r| format!("{}/{}", r.candidate(), r.file_name()))
            .collect();
        assert_eq!(
            names,
            [
                "zzz/20261008T140211Z-register-zzz.yaml",
                "aaa/20261009T081500Z-release-1.0.yaml",
                "bbb/20261009T081500Z-release-1.0.yaml",
            ]
        );
    }

    #[test]
    fn receipted_requests_are_not_pending() {
        let root = TempDir::new().unwrap();
        touch(
            root.path(),
            "requests/jpx/20261008T140211Z-register-jpx.yaml",
        );
        touch(
            root.path(),
            "requests/jpx/20261009T081500Z-release-1.0.yaml",
        );
        touch(
            root.path(),
            ".applied/jpx/20261008T140211Z-register-jpx.yaml",
        );
        let pending = pending_requests(root.path()).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].slug(), "1.0");
    }

    #[test]
    fn pending_fails_on_a_misnamed_file() {
        let root = TempDir::new().unwrap();
        touch(root.path(), "requests/jpx/1.2.0.yaml");
        let error = pending_requests(root.path()).unwrap_err();
        assert!(error.to_string().contains("requests/jpx/1.2.0.yaml"));
    }

    #[test]
    fn latest_applied_is_the_greatest_timestamp() {
        let root = TempDir::new().unwrap();
        touch(
            root.path(),
            ".applied/jpx/20261008T140211Z-register-jpx.yaml",
        );
        touch(
            root.path(),
            ".applied/jpx/20261014T094057Z-release-1.2.0.yaml",
        );
        touch(
            root.path(),
            ".applied/jpx/20261009T081500Z-release-1.1.0.yaml",
        );
        touch(
            root.path(),
            ".applied/other/20271231T000000Z-release-9.yaml",
        );
        let latest = latest_applied(root.path(), "jpx").unwrap().unwrap();
        assert_eq!(latest.to_string(), "20261014T094057Z");
    }

    #[test]
    fn write_receipt_creates_the_mirrored_file_and_skips_none_fields() {
        let root = TempDir::new().unwrap();
        let path = request("requests/jpx/20261014T094057Z-release-1.2.0.yaml");
        let mut written = receipt();
        written.rows = Some(vec![ReceiptRow {
            version: "1.2.0".into(),
            platform: "LINUX_X64".into(),
            sha256: "9c1e".into(),
            md5: None,
            sha512: None,
        }]);
        written.tag = Some(StableTag {
            stable: "1.2.0".into(),
        });
        write_receipt(root.path(), &path, &written).unwrap();
        let text = fs::read_to_string(root.path().join(path.receipt_path())).unwrap();
        assert!(!text.contains("md5"));
        assert!(!text.contains("register"));
        assert!(!text.contains("voided"));
        let read: Receipt = serde_norway::from_str(&text).unwrap();
        assert_eq!(read, written);
    }

    #[test]
    fn write_receipt_never_overwrites() {
        let root = TempDir::new().unwrap();
        let path = request("requests/jpx/20261008T140211Z-register-jpx.yaml");
        write_receipt(root.path(), &path, &receipt()).unwrap();
        let error = write_receipt(root.path(), &path, &receipt()).unwrap_err();
        assert!(matches!(
            error,
            ReceiptError::Io(ref e) if e.kind() == io::ErrorKind::AlreadyExists
        ));
    }

    #[test]
    fn voided_receipt_round_trips_without_rows_or_tag() {
        let root = TempDir::new().unwrap();
        let path = request("requests/jpx/20261008T140211Z-register-jpx.yaml");
        let mut voided = receipt();
        voided.voided = Some("voided by marco".into());
        write_receipt(root.path(), &path, &voided).unwrap();
        let text = fs::read_to_string(root.path().join(path.receipt_path())).unwrap();
        assert!(text.contains("voided: voided by marco"));
        assert!(!text.contains("rows"));
        assert!(!text.contains("tag"));
    }
}
