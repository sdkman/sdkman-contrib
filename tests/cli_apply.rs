mod support;

use assert_cmd::Command;
use predicates::prelude::*;
use support::fake_state::FakeState;
use support::repo::TempRepo;

const REGISTRATION_PATH: &str = "requests/jpx/20261008T120000Z-register-jpx.yaml";
const REGISTRATION: &str = "candidate: jpx\nregister:\n  name: JPX\n  description: JPX is a tool.\n  website: https://jpx.dev/\n";
const LOGIN_OK: (u16, &str) = (200, r#"{"token":"t1"}"#);

fn contrib(repo: &TempRepo, state: &FakeState) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("contrib"));
    command
        .current_dir(repo.path())
        .env("CONTRIB_STATE_URL", &state.url)
        .env("COMMUNITY_EMAIL", "bot@example.com")
        .env("COMMUNITY_PASSWORD", "secret");
    command
}

fn repo_with_pending_registration() -> TempRepo {
    let repo = TempRepo::with_remote();
    repo.write("contrib-candidates.txt", "jpx\n");
    repo.write(REGISTRATION_PATH, REGISTRATION);
    repo.commit_all("add request");
    repo.push();
    repo
}

fn paths(state: &FakeState) -> Vec<String> {
    state.requests().into_iter().map(|r| r.path).collect()
}

#[test]
fn apply_pushes_registration_receipt() {
    let repo = repo_with_pending_registration();
    let state = FakeState::answering(&[LOGIN_OK, (201, "")]);

    contrib(&repo, &state)
        .arg("apply")
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "applied {REGISTRATION_PATH}"
        )))
        .stderr(predicate::str::contains(format!(
            "applying to {}",
            state.url
        )));

    assert_eq!(paths(&state), vec!["/login", "/admin/candidates"]);
    assert!(
        repo.remote_log()
            .contains(&format!("chore(apply): record {REGISTRATION_PATH}"))
    );
}

#[test]
fn apply_needs_credentials() {
    let repo = repo_with_pending_registration();
    let state = FakeState::answering(&[]);
    contrib(&repo, &state)
        .env("COMMUNITY_EMAIL", "")
        .env("COMMUNITY_PASSWORD", "")
        .arg("apply")
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "missing COMMUNITY_EMAIL or COMMUNITY_PASSWORD",
        ));
    assert!(state.requests().is_empty());
}

#[test]
fn apply_exits_1_when_login_fails() {
    let repo = repo_with_pending_registration();
    let state = FakeState::answering(&[(401, "")]);
    contrib(&repo, &state)
        .arg("apply")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("login failed: State answered 401"));
    assert_eq!(paths(&state), vec!["/login"]);
}

#[test]
fn apply_exits_1_and_names_the_failing_request() {
    let repo = repo_with_pending_registration();
    let state = FakeState::answering(&[LOGIN_OK, (400, "")]);
    contrib(&repo, &state)
        .arg("apply")
        .assert()
        .code(1)
        .stderr(predicate::str::contains(REGISTRATION_PATH));
}

#[test]
fn void_records_a_receipt_and_writes_nothing_to_state() {
    let repo = repo_with_pending_registration();
    let state = FakeState::answering(&[LOGIN_OK]);

    contrib(&repo, &state)
        .args(["apply", "--void", REGISTRATION_PATH, "--reason", "mistake"])
        .assert()
        .success();

    assert_eq!(paths(&state), vec!["/login"]);
    assert!(
        repo.remote_log()
            .contains(&format!("chore(apply): void {REGISTRATION_PATH}"))
    );
}

#[test]
fn void_requires_a_reason() {
    let repo = repo_with_pending_registration();
    let state = FakeState::answering(&[]);
    contrib(&repo, &state)
        .args(["apply", "--void", REGISTRATION_PATH])
        .assert()
        .code(2);
}
