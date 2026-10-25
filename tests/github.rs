mod support;

use std::io::{Cursor, Write};

use sdkman_contrib::check_digests::{CheckDigests, DigestError, DigestRow, DigestSource};
use sdkman_contrib::github::GitHubDigests;
use serde_json::json;
use support::fake_github::{FakeGitHub, Reply};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

const REPOSITORY: &str = "sdkman/sdkman-contrib";
const COMMIT: &str = "c0ffee0000000000000000000000000000000000";
const HEAD: &str = "abcdef1234567890abcdef1234567890abcdef12";
const TOKEN: &str = "ghs_secret";

fn api(path: &str) -> String {
    format!("/repos/{REPOSITORY}/{path}")
}

fn reviewed() -> CheckDigests {
    CheckDigests {
        request: "requests/jpx/1.2.0.yaml".to_owned(),
        rows: vec![DigestRow {
            version: "1.2.0".to_owned(),
            platform: "LINUX_X64".to_owned(),
            sha256: "ab".repeat(32),
        }],
    }
}

fn zip_holding(name: &str, content: &str) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file(name, SimpleFileOptions::default())
        .unwrap();
    writer.write_all(content.as_bytes()).unwrap();
    writer.finish().unwrap().into_inner()
}

fn digests_zip() -> Vec<u8> {
    zip_holding("digests.json", &serde_json::to_string(&reviewed()).unwrap())
}

fn pulls() -> Reply {
    Reply::json(
        &json!([
            { "number": 3, "merged_at": null, "base": { "ref": "main" }, "head": { "sha": "0".repeat(40) } },
            { "number": 4, "merged_at": "2026-10-25T10:00:00Z", "base": { "ref": "release" }, "head": { "sha": "1".repeat(40) } },
            { "number": 7, "merged_at": "2026-10-25T10:00:00Z", "base": { "ref": "main" }, "head": { "sha": HEAD } },
        ])
        .to_string(),
    )
}

fn artifact_name() -> String {
    format!("digests-7-{HEAD}")
}

fn artifacts(entries: &[(u64, bool, &str, u64)]) -> Reply {
    let listed: Vec<_> = entries
        .iter()
        .map(|(id, expired, created_at, run)| {
            json!({
                "id": id,
                "expired": expired,
                "created_at": created_at,
                "workflow_run": { "id": run },
            })
        })
        .collect();
    Reply::json(&json!({ "total_count": listed.len(), "artifacts": listed }).to_string())
}

fn run(event: &str, conclusion: &str, path: &str) -> Reply {
    Reply::json(&json!({ "event": event, "conclusion": conclusion, "path": path }).to_string())
}

fn trusted_run() -> Reply {
    run(
        "pull_request_target",
        "success",
        ".github/workflows/request-check.yml@refs/heads/main",
    )
}

fn lookup(github: &FakeGitHub) -> Result<CheckDigests, DigestError> {
    GitHubDigests::new(&github.url, REPOSITORY, TOKEN).reviewed_digests(COMMIT)
}

fn commit_pulls() -> String {
    api(&format!("commits/{COMMIT}/pulls"))
}

fn named_artifacts() -> String {
    api(&format!("actions/artifacts?name={}", artifact_name()))
}

#[test]
fn trusted_artifact_is_read() {
    let github = FakeGitHub::routed(vec![
        (&commit_pulls(), pulls()),
        (
            &named_artifacts(),
            artifacts(&[(70, false, "2026-10-25T09:00:00Z", 900)]),
        ),
        (&api("actions/runs/900"), trusted_run()),
        (
            &api("actions/artifacts/70/zip"),
            Reply::redirect("/blob/70"),
        ),
        ("/blob/70", Reply::bytes(digests_zip())),
    ]);
    assert_eq!(lookup(&github).unwrap(), reviewed());
}

#[test]
fn zip_is_fetched_with_the_token_and_the_redirect_without_it() {
    let github = FakeGitHub::routed(vec![
        (&commit_pulls(), pulls()),
        (
            &named_artifacts(),
            artifacts(&[(70, false, "2026-10-25T09:00:00Z", 900)]),
        ),
        (&api("actions/runs/900"), trusted_run()),
        (
            &api("actions/artifacts/70/zip"),
            Reply::redirect("/blob/70"),
        ),
        ("/blob/70", Reply::bytes(digests_zip())),
    ]);
    lookup(&github).unwrap();
    let seen = github.seen();
    let bearer = Some(format!("Bearer {TOKEN}"));
    assert!(
        seen.iter()
            .filter(|s| s.path.starts_with("/repos/"))
            .all(|s| s.authorization == bearer)
    );
    let blob = seen.iter().find(|s| s.path == "/blob/70").unwrap();
    assert_eq!(blob.authorization, None);
}

#[test]
fn pull_request_artifact_is_never_read() {
    let github = FakeGitHub::routed(vec![
        (&commit_pulls(), pulls()),
        (
            &named_artifacts(),
            artifacts(&[(70, false, "2026-10-25T09:00:00Z", 900)]),
        ),
        (
            &api("actions/runs/900"),
            run(
                "pull_request",
                "success",
                ".github/workflows/request-check.yml@refs/pull/7/merge",
            ),
        ),
        (
            &api("actions/artifacts/70/zip"),
            Reply::redirect("/blob/70"),
        ),
        ("/blob/70", Reply::bytes(digests_zip())),
    ]);
    assert_eq!(lookup(&github), Err(DigestError::Missing));
    assert!(
        !github
            .paths()
            .iter()
            .any(|path| path.contains("/zip") || path.starts_with("/blob"))
    );
}

#[test]
fn run_of_another_workflow_or_without_success_is_not_trusted() {
    for (conclusion, path) in [
        (
            "failure",
            ".github/workflows/request-check.yml@refs/heads/main",
        ),
        ("success", ".github/workflows/other.yml@refs/heads/main"),
    ] {
        let github = FakeGitHub::routed(vec![
            (&commit_pulls(), pulls()),
            (
                &named_artifacts(),
                artifacts(&[(70, false, "2026-10-25T09:00:00Z", 900)]),
            ),
            (
                &api("actions/runs/900"),
                run("pull_request_target", conclusion, path),
            ),
        ]);
        assert_eq!(
            lookup(&github),
            Err(DigestError::Missing),
            "{conclusion} {path}"
        );
    }
}

#[test]
fn expired_artifact_is_missing() {
    let github = FakeGitHub::routed(vec![
        (&commit_pulls(), pulls()),
        (
            &named_artifacts(),
            artifacts(&[(70, true, "2026-07-01T09:00:00Z", 900)]),
        ),
        (&api("actions/runs/900"), trusted_run()),
    ]);
    assert_eq!(lookup(&github), Err(DigestError::Missing));
    assert!(
        !github
            .paths()
            .iter()
            .any(|path| path.contains("actions/runs"))
    );
}

#[test]
fn newest_trusted_artifact_wins_and_untrusted_ones_are_skipped() {
    let stale = zip_holding(
        "digests.json",
        &json!({ "request": "requests/jpx/0.0.1.yaml", "rows": [] }).to_string(),
    );
    let github = FakeGitHub::routed(vec![
        (&commit_pulls(), pulls()),
        (
            &named_artifacts(),
            artifacts(&[
                (10, false, "2026-10-25T08:00:00Z", 100),
                (30, false, "2026-10-25T10:00:00Z", 300),
                (20, false, "2026-10-25T09:00:00Z", 200),
            ]),
        ),
        (
            &api("actions/runs/300"),
            run(
                "pull_request",
                "success",
                ".github/workflows/request-check.yml",
            ),
        ),
        (&api("actions/runs/200"), trusted_run()),
        (&api("actions/runs/100"), trusted_run()),
        (
            &api("actions/artifacts/20/zip"),
            Reply::redirect("/blob/20"),
        ),
        ("/blob/20", Reply::bytes(digests_zip())),
        (
            &api("actions/artifacts/10/zip"),
            Reply::redirect("/blob/10"),
        ),
        ("/blob/10", Reply::bytes(stale)),
    ]);
    assert_eq!(lookup(&github).unwrap(), reviewed());
}

#[test]
fn commit_without_a_merged_main_pull_is_missing() {
    let github = FakeGitHub::routed(vec![(
        &commit_pulls(),
        Reply::json(
            &json!([{ "number": 3, "merged_at": null, "base": { "ref": "main" }, "head": { "sha": HEAD } }])
                .to_string(),
        ),
    )]);
    assert_eq!(lookup(&github), Err(DigestError::Missing));
}

#[test]
fn no_artifact_of_that_name_is_missing() {
    let github = FakeGitHub::routed(vec![
        (&commit_pulls(), pulls()),
        (&named_artifacts(), artifacts(&[])),
    ]);
    assert_eq!(lookup(&github), Err(DigestError::Missing));
}

#[test]
fn unexpected_status_is_a_failed_lookup() {
    let github = FakeGitHub::routed(vec![(&commit_pulls(), Reply::status(500))]);
    assert!(matches!(lookup(&github), Err(DigestError::Failed(_))));
}

#[test]
fn unreachable_api_is_a_failed_lookup() {
    let result =
        GitHubDigests::new("http://127.0.0.1:1", REPOSITORY, TOKEN).reviewed_digests(COMMIT);
    assert!(matches!(result, Err(DigestError::Failed(_))));
}

#[test]
fn artifact_without_digests_json_is_a_failed_lookup() {
    let github = FakeGitHub::routed(vec![
        (&commit_pulls(), pulls()),
        (
            &named_artifacts(),
            artifacts(&[(70, false, "2026-10-25T09:00:00Z", 900)]),
        ),
        (&api("actions/runs/900"), trusted_run()),
        (
            &api("actions/artifacts/70/zip"),
            Reply::redirect("/blob/70"),
        ),
        ("/blob/70", Reply::bytes(zip_holding("other.json", "{}"))),
    ]);
    assert!(matches!(lookup(&github), Err(DigestError::Failed(_))));
}
