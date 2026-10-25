mod support;

use assert_cmd::Command;
use predicates::prelude::*;
use support::fake_state::FakeState;
use support::repo::TempRepo;

const REGISTRATION_PATH: &str = "requests/jpx/20261008T120000Z-register-jpx.yaml";
const REGISTRATION: &str = "candidate: jpx\nregister:\n  name: JPX\n  description: JPX is a tool.\n  website: https://jpx.dev/\n";

fn repo_with_registration() -> TempRepo {
    let repo = TempRepo::new();
    repo.write("contrib-candidates.txt", "jpx\n");
    repo.write(REGISTRATION_PATH, REGISTRATION);
    repo
}

fn contrib(repo: &TempRepo, state: &FakeState) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("contrib"));
    command
        .current_dir(repo.path())
        .env("CONTRIB_STATE_URL", &state.url);
    command
}

#[test]
fn bad_name_exits_1() {
    let repo = repo_with_registration();
    let state = FakeState::answering(&[]);
    contrib(&repo, &state)
        .args(["check", "requests/jpx/jpx.yaml"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("FAIL"));
}

#[test]
fn registration_check_passes() {
    let repo = repo_with_registration();
    let state = FakeState::answering(&[(204, "")]);
    contrib(&repo, &state)
        .args(["check", REGISTRATION_PATH])
        .assert()
        .success()
        .stdout(predicate::str::contains("OK"))
        .stderr(predicate::str::contains(format!(
            "validating against {}",
            state.url
        )));
    assert_eq!(state.requests()[0].path, "/validate/candidates");
}

#[test]
fn normalises_file_argument() {
    let repo = repo_with_registration();
    let state = FakeState::answering(&[(204, "")]);
    let windows_path = format!(".\\{}", REGISTRATION_PATH.replace('/', "\\"));
    contrib(&repo, &state)
        .args(["check", &windows_path])
        .assert()
        .success();
}

#[test]
fn requires_file_or_base() {
    let repo = repo_with_registration();
    let state = FakeState::answering(&[]);
    contrib(&repo, &state).arg("check").assert().code(2);
}

#[test]
fn base_takes_the_added_request() {
    let repo = TempRepo::new();
    repo.write("contrib-candidates.txt", "jpx\n");
    let base = repo.commit_all("base");
    repo.write(REGISTRATION_PATH, REGISTRATION);
    repo.commit_all("add request");
    let state = FakeState::answering(&[(204, "")]);
    contrib(&repo, &state)
        .args(["check", "--base", &base])
        .assert()
        .success()
        .stdout(predicate::str::contains("jpx registration"));
}

#[test]
fn base_refuses_a_differing_file() {
    let repo = TempRepo::new();
    repo.write("contrib-candidates.txt", "jpx\n");
    let base = repo.commit_all("base");
    repo.write(REGISTRATION_PATH, REGISTRATION);
    repo.commit_all("add request");
    let state = FakeState::answering(&[]);
    contrib(&repo, &state)
        .args(["check", "requests/jpx/other.yaml", "--base", &base])
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "is not the request this change adds",
        ));
}

#[test]
fn base_without_an_added_request_exits_1() {
    let repo = TempRepo::new();
    repo.write("contrib-candidates.txt", "jpx\n");
    let base = repo.commit_all("base");
    repo.write("README.md", "hello\n");
    repo.commit_all("edit readme");
    let state = FakeState::answering(&[]);
    contrib(&repo, &state)
        .args(["check", "--base", &base])
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "a request PR adds exactly one file under requests/",
        ));
}

#[test]
fn report_file_holds_the_markdown() {
    let repo = repo_with_registration();
    let state = FakeState::answering(&[(204, "")]);
    contrib(&repo, &state)
        .args(["check", REGISTRATION_PATH, "--report", "report.md"])
        .assert()
        .success();
    let report = std::fs::read_to_string(repo.path().join("report.md")).unwrap();
    assert!(report.starts_with("## contrib check: PASS\n"));
}
