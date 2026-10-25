use std::fmt;

use chrono::{DateTime, SecondsFormat, Utc};

use crate::fetch::ArchiveSource;
use crate::receipt::{Receipt, ReceiptRow, RegisteredMetadata, StableTag};
use crate::request::{Registration, Release, Request};
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
    Write { subject: String, error: WriteError },
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected(messages) => f.write_str(&messages.join("; ")),
            Self::Write { subject, error } => write!(f, "{subject}: {error}"),
        }
    }
}

impl std::error::Error for ApplyError {}

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
