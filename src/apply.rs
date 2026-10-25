use std::fmt;
use std::path::Path;

use chrono::{DateTime, SecondsFormat, Utc};

use crate::check::admit_timestamp;
use crate::check_digests::{DigestError, DigestRow, DigestSource};
use crate::fetch::ArchiveSource;
use crate::git::Git;
use crate::load::load_request;
use crate::receipt::{
    Receipt, ReceiptError, ReceiptRow, RegisteredMetadata, StableTag, pending_requests,
    write_receipt,
};
use crate::report::row_subject;
use crate::request::{Registration, Release, Request};
use crate::request_path::RequestPath;
use crate::state_api::{StateWriter, WriteError};
use crate::state_body::{candidate_body, version_body};
use crate::verify::{RowCheck, verify_release};

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
    Blocked(Vec<String>),
    Failed(String),
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
            Self::Rejected(messages) | Self::Blocked(messages) => f.write_str(&messages.join("; ")),
            Self::Failed(message) => f.write_str(message),
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
    digests: &dyn DigestSource,
    git: &Git,
    run: &RunInfo,
) -> Result<Vec<String>, ApplyError> {
    let pending = pending_requests(repo_root).map_err(|error| ApplyError::Request {
        path: "requests".to_owned(),
        cause: Box::new(match error {
            ReceiptError::Naming { .. } => rejected(error),
            _ => failed(error),
        }),
    })?;
    let mut applied = Vec::new();
    for pending_path in pending {
        let repo_path = pending_path.repo_path();
        apply_pending(repo_root, &repo_path, writer, archives, digests, git, run).map_err(
            |cause| ApplyError::Request {
                path: repo_path.clone(),
                cause: Box::new(cause),
            },
        )?;
        applied.push(repo_path);
    }
    Ok(applied)
}

fn apply_pending(
    repo_root: &Path,
    repo_path: &str,
    writer: &dyn StateWriter,
    archives: &dyn ArchiveSource,
    digests: &dyn DigestSource,
    git: &Git,
    run: &RunInfo,
) -> Result<(), ApplyError> {
    let (path, request) = load_request(repo_root, repo_path).map_err(ApplyError::Rejected)?;
    admit_timestamp(repo_root, &path, Utc::now()).map_err(rejected)?;
    let commit = git.added_in(repo_path).map_err(failed)?;
    let merge_commit = git.added_in_full(repo_path).map_err(failed)?;
    let receipt = apply_request(
        &request,
        writer,
        archives,
        digests,
        Commits {
            receipt: &commit,
            merge: &merge_commit,
        },
        run,
    )?;
    record_receipt(repo_root, git, &path, &receipt, "record")
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
    if repo_root.join(path.receipt_path()).exists() {
        return Err(rejected("already has a receipt"));
    }
    let commit = git.added_in(repo_path).map_err(failed)?;
    let receipt = Receipt {
        voided: Some(reason.to_owned()),
        ..blank_receipt(run, &commit)
    };
    record_receipt(repo_root, git, &path, &receipt, "void")
}

fn record_receipt(
    repo_root: &Path,
    git: &Git,
    path: &RequestPath,
    receipt: &Receipt,
    verb: &str,
) -> Result<(), ApplyError> {
    write_receipt(repo_root, path, receipt).map_err(failed)?;
    git.commit_and_push(
        &path.receipt_path(),
        &format!("chore(apply): {verb} {}", path.repo_path()),
    )
    .map_err(failed)
}

fn blank_receipt(run: &RunInfo, commit: &str) -> Receipt {
    Receipt {
        applied_at: run.applied_at.clone(),
        commit: commit.to_owned(),
        run: run.run_url.clone(),
        rows: None,
        tag: None,
        register: None,
        voided: None,
    }
}

fn failed(error: impl fmt::Display) -> ApplyError {
    ApplyError::Failed(error.to_string())
}

fn rejected(error: impl fmt::Display) -> ApplyError {
    ApplyError::Rejected(vec![error.to_string()])
}

pub struct Commits<'a> {
    pub receipt: &'a str,
    pub merge: &'a str,
}

pub fn apply_request(
    request: &Request,
    writer: &dyn StateWriter,
    archives: &dyn ArchiveSource,
    digests: &dyn DigestSource,
    commits: Commits,
    run: &RunInfo,
) -> Result<Receipt, ApplyError> {
    let receipt = blank_receipt(run, commits.receipt);
    match request {
        Request::Registration(registration) => apply_registration(registration, writer, receipt),
        Request::Release(release) => {
            apply_release(release, writer, archives, digests, commits.merge, receipt)
        }
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
    digests: &dyn DigestSource,
    merge_commit: &str,
    receipt: Receipt,
) -> Result<Receipt, ApplyError> {
    let checks = verify_release(release, archives);
    let unread: Vec<String> = checks
        .iter()
        .filter(|row| row.digests.is_none())
        .flat_map(|row| row_messages(release, row))
        .collect();
    if !unread.is_empty() {
        return Err(ApplyError::Rejected(unread));
    }
    let reviewed = digests
        .reviewed_digests(merge_commit)
        .map_err(|error| match error {
            DigestError::Missing => ApplyError::Blocked(vec![error.to_string()]),
            DigestError::Failed(message) => ApplyError::Failed(message),
        })?;
    let changed: Vec<String> = checks
        .iter()
        .filter_map(|row| {
            let subject = row_subject(&release.candidate, &row.version, &row.platform);
            let reviewed_row = reviewed.rows.iter().find(|reviewed| {
                reviewed.version == row.version && reviewed.platform == row.platform
            });
            match (reviewed_row, DigestRow::from_check(row)) {
                (None, _) => Some(format!("{subject}: not among the reviewed digests")),
                (Some(reviewed), Some(current)) if reviewed.sha256 != current.sha256 => {
                    Some(format!("{subject}: archive changed since the check"))
                }
                _ => None,
            }
        })
        .collect();
    if !changed.is_empty() {
        return Err(ApplyError::Blocked(changed));
    }
    let failures: Vec<String> = checks
        .iter()
        .flat_map(|row| row_messages(release, row))
        .collect();
    if !failures.is_empty() {
        return Err(ApplyError::Rejected(failures));
    }
    let bodies = checks
        .iter()
        .map(|row| {
            version_body(release, row).ok_or_else(|| {
                ApplyError::Rejected(vec![format!(
                    "{}: no digests",
                    row_subject(&release.candidate, &row.version, &row.platform)
                )])
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    for body in &bodies {
        writer
            .post_version(body)
            .map_err(|error| ApplyError::Write {
                subject: row_subject(&body.candidate, &body.version, &body.platform),
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

fn row_messages(release: &Release, row: &RowCheck) -> Vec<String> {
    let subject = row_subject(&release.candidate, &row.version, &row.platform);
    row.failures
        .iter()
        .map(|failure| format!("{subject}: {failure}"))
        .collect()
}
