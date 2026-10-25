use std::fmt;
use std::path::Path;

use chrono::{DateTime, SecondsFormat, Utc};

use crate::check::admit_timestamp;
use crate::fetch::ArchiveSource;
use crate::git::Git;
use crate::load::load_request;
use crate::receipt::{
    Receipt, ReceiptRow, RegisteredMetadata, StableTag, pending_requests, write_receipt,
};
use crate::request::{Registration, Release, Request};
use crate::request_path::RequestPath;
use crate::state_api::{StateWriter, WriteError};
use crate::state_body::{candidate_body, version_body};
use crate::verify::verify_release;

const LOCAL_RUN: &str = "local";

pub struct RunInfo {
    pub applied_at: String,
    pub run_url: String,
}

impl RunInfo {
    pub fn from_env(now: DateTime<Utc>) -> Self {
        Self::from_vars(now, |name| std::env::var(name).ok())
    }

    pub fn from_vars(now: DateTime<Utc>, lookup: impl Fn(&str) -> Option<String>) -> Self {
        let get = |name: &str| lookup(name).filter(|value| !value.is_empty());
        let run_url = match (
            get("GITHUB_SERVER_URL"),
            get("GITHUB_REPOSITORY"),
            get("GITHUB_RUN_ID"),
        ) {
            (Some(server), Some(repository), Some(run_id)) => {
                format!("{server}/{repository}/actions/runs/{run_id}")
            }
            _ => LOCAL_RUN.to_owned(),
        };
        Self {
            applied_at: now.to_rfc3339_opts(SecondsFormat::Secs, true),
            run_url,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ApplyError {
    Rejected(Vec<String>),
    Write {
        subject: String,
        error: WriteError,
    },
    Request {
        path: String,
        cause: Box<ApplyError>,
    },
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected(messages) => f.write_str(&messages.join("; ")),
            Self::Write { subject, error } => write!(f, "{subject}: {error}"),
            Self::Request { path, cause } => write!(f, "{path}: {cause}"),
        }
    }
}

impl std::error::Error for ApplyError {}

pub fn apply_queue(
    repo_root: &Path,
    writer: &dyn StateWriter,
    archives: &dyn ArchiveSource,
    git: &Git,
    run: &RunInfo,
) -> Result<Vec<String>, ApplyError> {
    let pending = pending_requests(repo_root).map_err(|error| ApplyError::Request {
        path: "requests".to_owned(),
        cause: Box::new(rejected(error)),
    })?;
    let mut applied = Vec::new();
    for pending_path in pending {
        let repo_path = pending_path
            .repo_path()
            .to_string_lossy()
            .replace('\\', "/");
        apply_pending(repo_root, &repo_path, writer, archives, git, run).map_err(|cause| {
            ApplyError::Request {
                path: repo_path.clone(),
                cause: Box::new(cause),
            }
        })?;
        applied.push(repo_path);
    }
    Ok(applied)
}

fn apply_pending(
    repo_root: &Path,
    repo_path: &str,
    writer: &dyn StateWriter,
    archives: &dyn ArchiveSource,
    git: &Git,
    run: &RunInfo,
) -> Result<(), ApplyError> {
    let (path, request) = load_request(repo_root, repo_path).map_err(ApplyError::Rejected)?;
    admit_timestamp(repo_root, &path, Utc::now()).map_err(rejected)?;
    let commit = git.added_in(repo_path).map_err(rejected)?;
    let receipt = apply_request(&request, writer, archives, &commit, run)?;
    write_receipt(repo_root, &path, &receipt).map_err(rejected)?;
    let receipt_path = path.receipt_path().to_string_lossy().replace('\\', "/");
    git.commit_and_push(&receipt_path, &format!("chore(apply): record {repo_path}"))
        .map_err(rejected)
}

pub fn void_request(
    repo_root: &Path,
    git: &Git,
    run: &RunInfo,
    repo_path: &str,
    reason: &str,
) -> Result<(), ApplyError> {
    void_pending(repo_root, git, run, repo_path, reason).map_err(|cause| ApplyError::Request {
        path: repo_path.to_owned(),
        cause: Box::new(cause),
    })
}

fn void_pending(
    repo_root: &Path,
    git: &Git,
    run: &RunInfo,
    repo_path: &str,
    reason: &str,
) -> Result<(), ApplyError> {
    let path = RequestPath::parse(repo_path).map_err(rejected)?;
    if !repo_root.join(repo_path).is_file() {
        return Err(rejected("no such request"));
    }
    let receipt_path = path.receipt_path().to_string_lossy().replace('\\', "/");
    if repo_root.join(&receipt_path).exists() {
        return Err(rejected("already has a receipt"));
    }
    let commit = git.added_in(repo_path).map_err(rejected)?;
    let receipt = Receipt {
        applied_at: run.applied_at.clone(),
        commit,
        run: run.run_url.clone(),
        rows: None,
        tag: None,
        register: None,
        voided: Some(reason.to_owned()),
    };
    write_receipt(repo_root, &path, &receipt).map_err(rejected)?;
    git.commit_and_push(&receipt_path, &format!("chore(apply): void {repo_path}"))
        .map_err(rejected)
}

fn rejected(error: impl fmt::Display) -> ApplyError {
    ApplyError::Rejected(vec![error.to_string()])
}

pub fn apply_request(
    request: &Request,
    writer: &dyn StateWriter,
    archives: &dyn ArchiveSource,
    commit: &str,
    run: &RunInfo,
) -> Result<Receipt, ApplyError> {
    let receipt = Receipt {
        applied_at: run.applied_at.clone(),
        commit: commit.to_owned(),
        run: run.run_url.clone(),
        rows: None,
        tag: None,
        register: None,
        voided: None,
    };
    match request {
        Request::Registration(registration) => apply_registration(registration, writer, receipt),
        Request::Release(release) => apply_release(release, writer, archives, receipt),
    }
}

fn apply_registration(
    registration: &Registration,
    writer: &dyn StateWriter,
    receipt: Receipt,
) -> Result<Receipt, ApplyError> {
    writer
        .post_candidate(&candidate_body(registration))
        .map_err(|error| ApplyError::Write {
            subject: format!("{} registration", registration.candidate),
            error,
        })?;
    Ok(Receipt {
        register: Some(RegisteredMetadata {
            name: registration.name.clone(),
            description: registration.description.clone(),
            website: registration.website.clone(),
        }),
        ..receipt
    })
}

fn apply_release(
    release: &Release,
    writer: &dyn StateWriter,
    archives: &dyn ArchiveSource,
    receipt: Receipt,
) -> Result<Receipt, ApplyError> {
    let checks = verify_release(release, archives);
    let failures: Vec<String> = checks
        .iter()
        .flat_map(|row| {
            row.failures.iter().map(|failure| {
                format!(
                    "{} {} {}: {failure}",
                    release.candidate, row.version, row.platform
                )
            })
        })
        .collect();
    if !failures.is_empty() {
        return Err(ApplyError::Rejected(failures));
    }
    let bodies = checks
        .iter()
        .map(|row| {
            version_body(release, row).ok_or_else(|| {
                ApplyError::Rejected(vec![format!(
                    "{} {} {}: no digests",
                    release.candidate, row.version, row.platform
                )])
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    for body in &bodies {
        writer
            .post_version(body)
            .map_err(|error| ApplyError::Write {
                subject: format!("{} {} {}", body.candidate, body.version, body.platform),
                error,
            })?;
    }
    Ok(Receipt {
        rows: Some(
            bodies
                .iter()
                .map(|body| ReceiptRow {
                    version: body.version.clone(),
                    platform: body.platform.clone(),
                    sha256: body.sha256sum.clone(),
                    md5: body.md5sum.clone(),
                    sha512: body.sha512sum.clone(),
                })
                .collect(),
        ),
        tag: release.default.as_ref().map(|version| StableTag {
            stable: version.clone(),
        }),
        ..receipt
    })
}
