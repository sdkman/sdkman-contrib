use std::path::Path;

use chrono::{DateTime, Utc};

use crate::fetch::ArchiveSource;
use crate::load::load_request;
use crate::receipt::latest_applied;
use crate::report::{Report, ReportLine, Status, row_line};
use crate::request::{Registration, Release, Request};
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

pub fn run_check(context: &CheckContext, repo_path: &str) -> Report {
    let mut report = Report::default();
    let (path, request) = match load_request(context.repo_root, repo_path) {
        Ok(loaded) => loaded,
        Err(messages) => return failed(repo_path, messages),
    };
    if let Err(message) = admit_timestamp(context.repo_root, &path, context.now) {
        return failed(repo_path, vec![message]);
    }
    match &request {
        Request::Registration(registration) => {
            report.push(check_registration(context, registration));
        }
        Request::Release(release) => check_release(context, release, &mut report),
    }
    report
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

fn check_release(context: &CheckContext, release: &Release, report: &mut Report) {
    for row in verify_release(release, context.archives) {
        let failures = match version_body(release, &row) {
            Some(body) => state_failures(context.validator.validate_version(&body)),
            None => Vec::new(),
        };
        report.push(row_line(&release.candidate, &row, &failures));
    }
}

fn state_failures(answer: Result<Answer, String>) -> Vec<String> {
    match answer {
        Ok(answer) if answer.passed() => Vec::new(),
        Ok(answer) => answer.messages(),
        Err(error) => vec![format!("State could not be reached: {error}")],
    }
}
