mod support;

use chrono::{DateTime, Utc};
use sdkman_contrib::check::admit_timestamp;
use sdkman_contrib::load::load_request;
use sdkman_contrib::receipt::{Receipt, write_receipt};
use sdkman_contrib::request_path::{RequestPath, Timestamp};
use support::repo::TempRepo;

const REGISTRATION_PATH: &str = "requests/jpx/20261008T120000Z-register-jpx.yaml";
const RELEASE_PATH: &str = "requests/jpx/20261008T130000Z-release-jpx-1.2.0.yaml";
const REGISTRATION: &str = "candidate: jpx\nregister:\n  name: JPX\n  description: JPX is a tool.\n  website: https://jpx.dev/\n";
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
