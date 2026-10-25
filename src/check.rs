use std::path::Path;

use chrono::{DateTime, Utc};

use crate::check_digests::{CheckDigests, DigestRow};
use crate::fetch::ArchiveSource;
use crate::load::load_request;
use crate::receipt::latest_applied;
use crate::report::{Report, ReportLine, Status, row_line};
use crate::request::{LINUX_X64, Registration, Release, Request, UNIVERSAL};
use crate::request_path::{RequestPath, check_timestamp};
use crate::state_api::{Answer, StateValidator};
use crate::state_body::{candidate_body, version_body};
use crate::verify::verify_release;

pub struct CheckContext<'a> {
    pub repo_root: &'a Path,
    pub now: DateTime<Utc>,
    pub validator: &'a dyn StateValidator,
    pub archives: &'a dyn ArchiveSource,
}

pub fn admit_timestamp(
    repo_root: &Path,
    path: &RequestPath,
    now: DateTime<Utc>,
) -> Result<(), String> {
    let latest = latest_applied(repo_root, path.candidate())
        .map_err(|error| format!("cannot read the receipts of {}: {error}", path.candidate()))?;
    check_timestamp(&path.timestamp(), now, latest.as_ref())
}

pub struct CheckOutcome {
    pub report: Report,
    pub digests: CheckDigests,
}

pub fn run_check(context: &CheckContext, repo_path: &str) -> Report {
    check_request(context, repo_path).report
}

pub fn check_request(context: &CheckContext, repo_path: &str) -> CheckOutcome {
    let mut report = Report::default();
    let mut rows = Vec::new();
    match check_loaded(context, repo_path, &mut report) {
        Ok(checked) => rows = checked,
        Err(messages) => report = failed(repo_path, messages),
    }
    CheckOutcome {
        report,
        digests: CheckDigests {
            request: repo_path.to_owned(),
            rows,
        },
    }
}

fn check_loaded(
    context: &CheckContext,
    repo_path: &str,
    report: &mut Report,
) -> Result<Vec<DigestRow>, Vec<String>> {
    let (path, request) = load_request(context.repo_root, repo_path)?;
    admit_timestamp(context.repo_root, &path, context.now).map_err(|message| vec![message])?;
    Ok(match &request {
        Request::Registration(registration) => {
            report.push(check_registration(context, registration));
            Vec::new()
        }
        Request::Release(release) => check_release(context, release, report),
    })
}

fn failed(subject: &str, messages: Vec<String>) -> Report {
    let mut report = Report::default();
    for message in messages {
        report.push(ReportLine {
            status: Status::Fail,
            subject: subject.to_owned(),
            detail: message,
        });
    }
    report
}

fn check_registration(context: &CheckContext, registration: &Registration) -> ReportLine {
    let answer = context
        .validator
        .validate_candidate(&candidate_body(registration));
    let failures = state_failures(answer);
    ReportLine {
        status: if failures.is_empty() {
            Status::Ok
        } else {
            Status::Fail
        },
        subject: format!("{} registration", registration.candidate),
        detail: failures.join("; "),
    }
}

fn check_release(context: &CheckContext, release: &Release, report: &mut Report) -> Vec<DigestRow> {
    let mut digests = Vec::new();
    for row in verify_release(release, context.archives) {
        digests.extend(DigestRow::from_check(&row));
        let failures = match version_body(release, &row) {
            Some(body) => state_failures(context.validator.validate_version(&body)),
            None => Vec::new(),
        };
        report.push(row_line(&release.candidate, &row, &failures));
    }
    if let Some(line) = unmovable_default_warning(release) {
        report.push(line);
    }
    digests
}

fn unmovable_default_warning(release: &Release) -> Option<ReportLine> {
    let default = release.default.as_ref()?;
    let version = release
        .versions
        .iter()
        .find(|version| &version.version == default)?;
    let movable = [UNIVERSAL, LINUX_X64]
        .iter()
        .any(|platform| version.platforms.contains_key(*platform));
    if movable {
        return None;
    }
    Some(ReportLine {
        status: Status::Warn,
        subject: format!("{} {default} default", release.candidate),
        detail: format!(
            "the default will not move without a {UNIVERSAL} or {LINUX_X64} row; \
             State clears stable from other versions' {UNIVERSAL} rows, so a {UNIVERSAL} default is lost"
        ),
    })
}

fn state_failures(answer: Result<Answer, String>) -> Vec<String> {
    match answer {
        Ok(answer) if answer.passed() => Vec::new(),
        Ok(answer) => answer.messages(),
        Err(error) => vec![format!("State call failed: {error}")],
    }
}
