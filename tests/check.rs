mod support;

use chrono::{DateTime, Utc};
use sdkman_contrib::check::{CheckContext, admit_timestamp, run_check};
use sdkman_contrib::fetch::ArchiveSource;
use sdkman_contrib::load::load_request;
use sdkman_contrib::receipt::{Receipt, write_receipt};
use sdkman_contrib::request_path::{RequestPath, Timestamp};
use sdkman_contrib::state_api::Answer;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::Path;
use support::archives::write_zip;
use support::fakes::{FakeArchives, FakeHops, FakeValidator};
use support::fixtures::{GOOD_ENTRIES_1_2_0 as GOOD_ENTRIES, REGISTRATION, REGISTRATION_PATH};
use support::repo::TempRepo;
use tempfile::TempDir;

const RELEASE_PATH: &str = "requests/jpx/20261008T130000Z-release-jpx-1.2.0.yaml";
const RELEASE: &str = "candidate: jpx\ndefault: 1.2.0\nversions:\n  - version: 1.2.0\n    platforms:\n      LINUX_X64: https://downloads.jpx.dev/jpx-1.2.0.zip\n";

fn repo_listing(candidates: &str) -> TempRepo {
    let repo = TempRepo::new();
    repo.write("contrib-candidates.txt", candidates);
    repo
}

fn now_at(timestamp: &str) -> DateTime<Utc> {
    Timestamp::parse(timestamp).unwrap().at().and_utc()
}

fn receipt() -> Receipt {
    Receipt {
        applied_at: "2026-10-09T00:00:00Z".into(),
        commit: "abc123".into(),
        run: "1".into(),
        rows: None,
        tag: None,
        register: None,
        voided: None,
    }
}

#[test]
fn loads_a_listed_registration() {
    let repo = repo_listing("jpx\n");
    repo.write(REGISTRATION_PATH, REGISTRATION);
    let (path, request) = load_request(repo.path(), REGISTRATION_PATH).unwrap();
    assert_eq!(path.candidate(), "jpx");
    assert_eq!(request.candidate(), "jpx");
}

#[test]
fn loads_a_listed_release() {
    let repo = repo_listing("jpx\n");
    repo.write(RELEASE_PATH, RELEASE);
    assert!(load_request(repo.path(), RELEASE_PATH).is_ok());
}

#[test]
fn bad_name_fails_before_reading() {
    let repo = repo_listing("jpx\n");
    let errors = load_request(repo.path(), "requests/jpx/jpx.yaml").unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("requests/<candidate>/<timestamp>-<kind>-<slug>.yaml"));
}

#[test]
fn missing_file_fails() {
    let repo = repo_listing("jpx\n");
    let errors = load_request(repo.path(), REGISTRATION_PATH).unwrap_err();
    assert!(errors[0].contains("cannot read"));
}

#[test]
fn unknown_field_fails() {
    let repo = repo_listing("jpx\n");
    repo.write(REGISTRATION_PATH, &format!("{REGISTRATION}extra: 1\n"));
    assert!(load_request(repo.path(), REGISTRATION_PATH).is_err());
}

#[test]
fn java_is_refused() {
    let repo = repo_listing("java\n");
    let path = "requests/java/20261008T120000Z-register-java.yaml";
    repo.write(path, &REGISTRATION.replace("jpx", "java"));
    let errors = load_request(repo.path(), path).unwrap_err();
    assert_eq!(errors, vec!["java is refused: DISCO owns java"]);
}

#[test]
fn unlisted_candidate_is_refused() {
    let repo = repo_listing("# nobody yet\n");
    repo.write(REGISTRATION_PATH, REGISTRATION);
    let errors = load_request(repo.path(), REGISTRATION_PATH).unwrap_err();
    assert_eq!(errors, vec!["jpx is not listed in contrib-candidates.txt"]);
}

#[test]
fn missing_listing_fails() {
    let repo = TempRepo::new();
    repo.write(REGISTRATION_PATH, REGISTRATION);
    let errors = load_request(repo.path(), REGISTRATION_PATH).unwrap_err();
    assert!(errors[0].contains("cannot read the candidate listing"));
}

#[test]
fn mixed_kinds_fail() {
    let repo = repo_listing("jpx\n");
    repo.write(REGISTRATION_PATH, RELEASE);
    let errors = load_request(repo.path(), REGISTRATION_PATH).unwrap_err();
    assert!(errors[0].contains("file name says `register` but the content is a `release`"));
}

#[test]
fn directory_mismatch_fails() {
    let repo = repo_listing("jpx\n");
    let path = "requests/other/20261008T120000Z-register-jpx.yaml";
    repo.write(path, REGISTRATION);
    let errors = load_request(repo.path(), path).unwrap_err();
    assert!(errors[0].contains("directory `other` does not match candidate `jpx`"));
}

#[test]
fn release_structure_errors_fail() {
    let repo = repo_listing("jpx\n");
    repo.write(
        RELEASE_PATH,
        &RELEASE.replace("default: 1.2.0", "default: 9.9"),
    );
    let errors = load_request(repo.path(), RELEASE_PATH).unwrap_err();
    assert_eq!(errors, vec!["default 9.9 is not a version in this request"]);
}

#[test]
fn backdated_request_fails() {
    let repo = repo_listing("jpx\n");
    let later = RequestPath::parse("requests/jpx/20261009T000000Z-release-later.yaml").unwrap();
    write_receipt(repo.path(), &later, &receipt()).unwrap();
    let path = RequestPath::parse(REGISTRATION_PATH).unwrap();
    let error = admit_timestamp(repo.path(), &path, now_at("20261010T000000Z")).unwrap_err();
    assert!(error.contains("earlier than the latest applied request 20261009T000000Z"));
}

#[test]
fn future_request_fails() {
    let repo = repo_listing("jpx\n");
    let path = RequestPath::parse(REGISTRATION_PATH).unwrap();
    let error = admit_timestamp(repo.path(), &path, now_at("20261008T110000Z")).unwrap_err();
    assert!(error.contains("is in the future"));
}

#[test]
fn request_after_latest_receipt_is_admitted() {
    let repo = repo_listing("jpx\n");
    let earlier = RequestPath::parse("requests/jpx/20261001T000000Z-release-earlier.yaml").unwrap();
    write_receipt(repo.path(), &earlier, &receipt()).unwrap();
    let path = RequestPath::parse(REGISTRATION_PATH).unwrap();
    assert!(admit_timestamp(repo.path(), &path, now_at("20261010T000000Z")).is_ok());
}

const ARCHIVE_URL: &str = "https://downloads.jpx.dev/jpx-1.2.0.zip";
const OTHER_URL: &str = "https://downloads.jpx.dev/jpx-1.3.0.zip";

fn refusal(message: &str) -> Result<Answer, String> {
    Ok(Answer {
        status: 400,
        body: json!({ "error": "validation", "failures": [{ "field": "candidate", "message": message }] }),
    })
}

struct Fixture {
    repo: TempRepo,
    archives: FakeArchives,
    dir: TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            repo: repo_listing("jpx\n"),
            archives: FakeArchives::default(),
            dir: TempDir::new().unwrap(),
        }
    }

    fn serving(mut self, url: &str, name: &str, entries: &[(&str, u32)]) -> Self {
        let file = self.dir.path().join(name);
        write_zip(&file, entries);
        self.archives = self.archives.serving(url, &file);
        self
    }

    fn check(&self, validator: &FakeValidator, path: &str) -> sdkman_contrib::report::Report {
        let context = CheckContext {
            repo_root: self.repo.path(),
            now: now_at("20261010T000000Z"),
            validator,
            archives: &self.archives,
        };
        run_check(&context, path)
    }
}

fn sha256_of(file: &Path) -> String {
    hex::encode(Sha256::digest(std::fs::read(file).unwrap()))
}

fn good_release() -> Fixture {
    let fixture = Fixture::new().serving(ARCHIVE_URL, "a.zip", &GOOD_ENTRIES);
    fixture.repo.write(RELEASE_PATH, RELEASE);
    fixture
}

#[test]
fn registration_posts_its_candidate_body() {
    let fixture = Fixture::new();
    fixture.repo.write(REGISTRATION_PATH, REGISTRATION);
    let validator = FakeValidator::default();
    let report = fixture.check(&validator, REGISTRATION_PATH);
    assert!(!report.has_failures());
    assert_eq!(report.render_text(), "OK    jpx registration\n");
    let bodies = validator.candidates();
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0].candidate, "jpx");
    assert_eq!(bodies[0].website_url, "https://jpx.dev/");
}

#[test]
fn refused_registration_fails_with_state_message() {
    let fixture = Fixture::new();
    fixture.repo.write(REGISTRATION_PATH, REGISTRATION);
    let validator = FakeValidator::default().then(refusal("name is blank"));
    let report = fixture.check(&validator, REGISTRATION_PATH);
    assert!(report.has_failures());
    assert_eq!(
        report.render_text(),
        "FAIL  jpx registration  candidate: name is blank\n"
    );
}

#[test]
fn unregistered_candidate_fails_each_row() {
    let second_url = "https://downloads.jpx.dev/jpx-1.2.0-mac.zip";
    let fixture = good_release().serving(second_url, "b.zip", &GOOD_ENTRIES);
    let release = RELEASE.replace(
        "      LINUX_X64: https://downloads.jpx.dev/jpx-1.2.0.zip\n",
        &format!("      LINUX_X64: {ARCHIVE_URL}\n      MAC_ARM64: {second_url}\n"),
    );
    fixture.repo.write(RELEASE_PATH, &release);
    let validator = FakeValidator::default()
        .then(refusal("jpx is not registered"))
        .then(refusal("jpx is not registered"));
    let report = fixture.check(&validator, RELEASE_PATH);
    let text = report.render_text();
    assert!(report.has_failures());
    assert_eq!(text.matches("FAIL").count(), 2, "{text}");
    assert!(text.contains("LINUX_X64"), "{text}");
    assert!(text.contains("MAC_ARM64"), "{text}");
    assert_eq!(
        text.matches("candidate: jpx is not registered").count(),
        2,
        "{text}"
    );
}

#[test]
fn passing_release_is_ok_and_posts_the_computed_sha256() {
    let fixture = good_release();
    let validator = FakeValidator::default();
    let report = fixture.check(&validator, RELEASE_PATH);
    assert!(!report.has_failures());
    assert!(
        report
            .render_text()
            .starts_with("OK    jpx 1.2.0 LINUX_X64")
    );
    let bodies = validator.versions();
    assert_eq!(bodies.len(), 1);
    assert_eq!(
        bodies[0].sha256sum,
        sha256_of(&fixture.dir.path().join("a.zip"))
    );
    assert!(report.render_text().contains(&bodies[0].sha256sum));
}

#[test]
fn missing_bin_fails_the_row() {
    let fixture = Fixture::new().serving(
        ARCHIVE_URL,
        "a.zip",
        &[("jpx-1.2.0/", 0o755), ("jpx-1.2.0/README", 0o644)],
    );
    fixture.repo.write(RELEASE_PATH, RELEASE);
    let report = fixture.check(&FakeValidator::default(), RELEASE_PATH);
    assert!(report.has_failures());
    assert!(
        report
            .render_text()
            .starts_with("FAIL  jpx 1.2.0 LINUX_X64")
    );
    assert!(report.render_text().contains("no bin/ under jpx-1.2.0/"));
}

#[test]
fn each_checksum_mismatch_fails_the_row() {
    for algorithm in ["md5", "sha256", "sha512"] {
        let fixture = good_release();
        let declared = format!(
            "      LINUX_X64:\n        url: {ARCHIVE_URL}\n        {algorithm}: {}\n",
            "0".repeat(8)
        );
        let release = RELEASE.replace(&format!("      LINUX_X64: {ARCHIVE_URL}\n"), &declared);
        fixture.repo.write(RELEASE_PATH, &release);
        let report = fixture.check(&FakeValidator::default(), RELEASE_PATH);
        assert!(report.has_failures(), "{algorithm}");
        assert!(
            report
                .render_text()
                .contains(&format!("declared {algorithm} does not match the download")),
            "{algorithm}"
        );
        assert!(
            report
                .render_text()
                .contains(&format!("{algorithm} declared, not matched")),
            "{algorithm}"
        );
    }
}

#[test]
fn undownloadable_archive_fails_without_a_state_call() {
    let fixture = Fixture::new();
    fixture.repo.write(RELEASE_PATH, RELEASE);
    let validator = FakeValidator::default();
    let report = fixture.check(&validator, RELEASE_PATH);
    assert!(report.has_failures());
    assert!(validator.versions().is_empty());
    assert!(report.render_text().contains("answered 404"));
}

#[test]
fn transport_error_is_one_state_failure() {
    let fixture = good_release();
    let validator = FakeValidator::default().then(Err("connection refused".to_owned()));
    let report = fixture.check(&validator, RELEASE_PATH);
    assert!(report.has_failures());
    assert!(
        report
            .render_text()
            .contains("State call failed: connection refused")
    );
}

#[test]
fn non_204_answer_without_messages_names_its_status() {
    let fixture = good_release();
    let validator = FakeValidator::answering(&[404]);
    let report = fixture.check(&validator, RELEASE_PATH);
    assert!(report.render_text().contains("State answered 404"));
}

#[test]
fn load_failure_is_one_fail_line_and_stops() {
    let fixture = Fixture::new();
    fixture.repo.write(REGISTRATION_PATH, REGISTRATION);
    let validator = FakeValidator::default();
    let report = fixture.check(&validator, "requests/jpx/jpx.yaml");
    assert_eq!(report.render_text().lines().count(), 1);
    assert!(
        report
            .render_text()
            .starts_with("FAIL  requests/jpx/jpx.yaml")
    );
    assert!(validator.candidates().is_empty());
}

#[test]
fn backdated_request_fails_the_check_without_a_state_call() {
    let fixture = Fixture::new();
    fixture.repo.write(REGISTRATION_PATH, REGISTRATION);
    let later = RequestPath::parse("requests/jpx/20261009T140000Z-release-later.yaml").unwrap();
    write_receipt(fixture.repo.path(), &later, &receipt()).unwrap();
    let validator = FakeValidator::default();
    let report = fixture.check(&validator, REGISTRATION_PATH);
    assert!(report.has_failures());
    assert!(
        report
            .render_text()
            .contains("earlier than the latest applied request")
    );
    assert!(validator.candidates().is_empty());
}

#[test]
fn default_rows_validate_with_stable_tag() {
    let fixture = Fixture::new()
        .serving(ARCHIVE_URL, "a.zip", &GOOD_ENTRIES)
        .serving(
            OTHER_URL,
            "b.zip",
            &[
                ("jpx-1.3.0/", 0o755),
                ("jpx-1.3.0/bin/", 0o755),
                ("jpx-1.3.0/bin/jpx", 0o755),
            ],
        );
    let release = format!(
        "candidate: jpx\ndefault: 1.2.0\nversions:\n  - version: 1.2.0\n    platforms:\n      LINUX_X64: {ARCHIVE_URL}\n  - version: 1.3.0\n    platforms:\n      LINUX_X64: {OTHER_URL}\n"
    );
    fixture.repo.write(RELEASE_PATH, &release);
    let validator = FakeValidator::default();
    let report = fixture.check(&validator, RELEASE_PATH);
    assert!(!report.has_failures());
    let bodies = validator.versions();
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[0].version, "1.2.0");
    assert_eq!(bodies[0].tags, Some(vec!["stable".to_owned()]));
    assert_eq!(bodies[1].version, "1.3.0");
    assert_eq!(bodies[1].tags, None);
}

const GITHUB_URL: &str = "https://github.com/jpx/jpx/releases/download/v1.2.0/jpx-1.2.0.zip";
const CDN_URL: &str = "https://objects.githubusercontent.com/jpx-1.2.0.zip";

fn zip_bytes(entries: &[(&str, u32)]) -> Vec<u8> {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("a.zip");
    write_zip(&file, entries);
    std::fs::read(file).unwrap()
}

fn github_release_repo() -> TempRepo {
    let repo = repo_listing("jpx\n");
    repo.write(RELEASE_PATH, &RELEASE.replace(ARCHIVE_URL, GITHUB_URL));
    repo
}

fn check_over(
    repo: &TempRepo,
    archives: &dyn ArchiveSource,
    validator: &FakeValidator,
) -> sdkman_contrib::report::Report {
    let context = CheckContext {
        repo_root: repo.path(),
        now: now_at("20261010T000000Z"),
        validator,
        archives,
    };
    run_check(&context, RELEASE_PATH)
}

#[test]
fn github_release_download_passes() {
    let repo = github_release_repo();
    let hops = FakeHops::default()
        .redirecting(GITHUB_URL, CDN_URL)
        .serving(CDN_URL, &zip_bytes(&GOOD_ENTRIES));
    let validator = FakeValidator::default();
    let report = check_over(&repo, &hops, &validator);
    assert!(!report.has_failures(), "{}", report.render_text());
    assert!(
        report
            .render_text()
            .starts_with("OK    jpx 1.2.0 LINUX_X64")
    );
    assert_eq!(hops.requested(), vec![GITHUB_URL, CDN_URL]);
    let bodies = validator.versions();
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0].url, GITHUB_URL);
}

#[test]
fn redirected_archive_is_judged() {
    let repo = github_release_repo();
    let hops = FakeHops::default()
        .redirecting(GITHUB_URL, CDN_URL)
        .serving(
            CDN_URL,
            &zip_bytes(&[("jpx-1.2.0/", 0o755), ("jpx-1.2.0/README", 0o644)]),
        );
    let report = check_over(&repo, &hops, &FakeValidator::default());
    assert!(report.has_failures());
    assert!(
        report
            .render_text()
            .starts_with("FAIL  jpx 1.2.0 LINUX_X64")
    );
    assert!(report.render_text().contains("no bin/ under jpx-1.2.0/"));
}
