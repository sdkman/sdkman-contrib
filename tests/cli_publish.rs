mod support;

use assert_cmd::Command;
use predicates::prelude::*;
use support::archives::{write_release, write_release_archive};
use support::fake_state::FakeState;
use tempfile::TempDir;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const URL_BASE: &str = "https://example.com/releases";
const LOGIN_OK: (u16, &str) = (200, r#"{"token":"t1"}"#);
const NOT_FOUND: (u16, &str) = (404, "");
const NO_CONTENT: (u16, &str) = (204, "");

fn contrib(dir: &TempDir, state: &FakeState) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("contrib"));
    command
        .env("CONTRIB_STATE_URL", &state.url)
        .env("VENDOR_EMAIL", "vendor@example.com")
        .env("VENDOR_PASSWORD", "secret")
        .args(["publish", "--url-base", URL_BASE])
        .arg(dir.path());
    command
}

fn release_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    write_release(dir.path(), VERSION, &format!("contrib-{VERSION}"));
    dir
}

fn reads_of_a_fresh_release() -> Vec<(u16, &'static str)> {
    let mut script = vec![LOGIN_OK];
    script.extend([NOT_FOUND; 10]);
    script
}

#[test]
fn publishes_five_rows_through_the_vendor_role() {
    let dir = release_dir();
    let mut script = reads_of_a_fresh_release();
    script.extend([NO_CONTENT; 10]);
    let state = FakeState::answering(&script);

    contrib(&dir, &state)
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "published contrib {VERSION}"
        )));

    let requests = state.requests();
    assert_eq!(requests[0].path, "/login");
    let writes: Vec<_> = requests
        .iter()
        .filter(|request| request.method == "POST" && request.path != "/login")
        .collect();
    assert_eq!(writes.len(), 10);
    assert!(
        writes[..5]
            .iter()
            .all(|request| request.path == "/versions")
    );
    assert!(
        writes[5..]
            .iter()
            .all(|request| request.path == "/versions/tags")
    );
    assert!(writes[9].body.contains("LINUX_X64"));
    assert!(
        writes
            .iter()
            .all(|request| request.authorization.as_deref() == Some("Bearer t1"))
    );
}

#[test]
fn malformed_archive_reaches_no_state() {
    let dir = release_dir();
    write_release_archive(dir.path(), VERSION, "linux-x64.tar.gz", "contrib");
    let state = FakeState::answering(&[]);

    contrib(&dir, &state)
        .assert()
        .code(1)
        .stderr(predicate::str::contains(format!(
            "contrib-{VERSION}-linux-x64.tar.gz"
        )));

    assert!(state.requests().is_empty());
}

#[test]
fn missing_vendor_credentials_reach_no_state() {
    let dir = release_dir();
    let state = FakeState::answering(&[]);

    contrib(&dir, &state)
        .env("VENDOR_PASSWORD", "")
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "missing VENDOR_EMAIL or VENDOR_PASSWORD",
        ));

    assert!(state.requests().is_empty());
}

#[test]
fn refused_row_fails_without_tagging() {
    let dir = release_dir();
    let mut script = reads_of_a_fresh_release();
    script.push((400, r#"{"message":"bad row"}"#));
    script.extend([NO_CONTENT; 10]);
    let state = FakeState::answering(&script);

    contrib(&dir, &state)
        .assert()
        .code(1)
        .stderr(predicate::str::contains("bad row"));

    assert!(
        state
            .requests()
            .iter()
            .all(|request| request.path != "/versions/tags")
    );
}
