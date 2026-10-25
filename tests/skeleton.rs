mod support;

use assert_cmd::Command;
use chrono::{DateTime, Utc};
use predicates::prelude::*;
use sdkman_contrib::check::{CheckContext, run_check};
use sdkman_contrib::request_path::{Kind, Timestamp};
use sdkman_contrib::skeleton::{
    registration_skeleton, release_skeleton, skeleton_path, write_skeleton,
};
use support::archives::write_zip;
use support::fakes::{FakeArchives, FakeValidator};
use support::repo::TempRepo;
use tempfile::TempDir;

fn now() -> DateTime<Utc> {
    Timestamp::parse("20261014T094057Z").unwrap().at().and_utc()
}

fn check(repo: &TempRepo, path: &str, archives: &FakeArchives) -> String {
    let validator = FakeValidator::default();
    let context = CheckContext {
        repo_root: repo.path(),
        now: now(),
        validator: &validator,
        archives,
    };
    run_check(&context, path).render_text()
}

#[test]
fn filled_skeleton_passes_check() {
    let repo = TempRepo::new();
    repo.write("contrib-candidates.txt", "jpx\n");

    let registration_path = skeleton_path("jpx", Kind::Register, "jpx", now()).unwrap();
    let registration = registration_skeleton("jpx")
        .replace("REPLACE_WITH_NAME", "JPX")
        .replace("REPLACE_WITH_DESCRIPTION", "JPX is a tool.")
        .replace("REPLACE_WITH_WEBSITE_URL", "https://jpx.dev/");
    write_skeleton(repo.path(), &registration_path, &registration).unwrap();
    let text = check(&repo, &registration_path, &FakeArchives::default());
    assert!(!text.contains("FAIL"), "{text}");
    assert!(text.contains("OK"), "{text}");

    let archive_dir = TempDir::new().unwrap();
    let archive = archive_dir.path().join("jpx-1.2.0.zip");
    write_zip(
        &archive,
        &[("jpx-1.2.0/bin/jpx", 0o755), ("jpx-1.2.0/README", 0o644)],
    );
    let url = "https://downloads.jpx.dev/jpx-1.2.0.zip";
    let platforms = vec!["LINUX_X64".to_owned()];
    let release_path = skeleton_path("jpx", Kind::Release, "1.2.0", now()).unwrap();
    let release =
        release_skeleton("jpx", "1.2.0", &platforms).replace("REPLACE_WITH_LINUX_X64_URL", url);
    write_skeleton(repo.path(), &release_path, &release).unwrap();
    let archives = FakeArchives::default().serving(url, &archive);
    let text = check(&repo, &release_path, &archives);
    assert!(!text.contains("FAIL"), "{text}");
    assert!(text.contains("OK"), "{text}");
}

#[test]
fn unfilled_release_skeleton_fails_check() {
    let repo = TempRepo::new();
    repo.write("contrib-candidates.txt", "jpx\n");
    let path = skeleton_path("jpx", Kind::Release, "1.2.0", now()).unwrap();
    let platforms = vec!["LINUX_X64".to_owned()];
    write_skeleton(
        repo.path(),
        &path,
        &release_skeleton("jpx", "1.2.0", &platforms),
    )
    .unwrap();
    assert!(check(&repo, &path, &FakeArchives::default()).contains("FAIL"));
}

#[test]
fn write_skeleton_refuses_an_existing_file() {
    let repo = TempRepo::new();
    let path = skeleton_path("jpx", Kind::Register, "jpx", now()).unwrap();
    write_skeleton(repo.path(), &path, "first").unwrap();
    let error = write_skeleton(repo.path(), &path, "second").unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(
        std::fs::read_to_string(repo.path().join(&path)).unwrap(),
        "first"
    );
}

#[test]
fn new_release_writes_a_stamped_file() {
    let repo = TempRepo::new();
    let output = Command::new(assert_cmd::cargo::cargo_bin!("contrib"))
        .current_dir(repo.path())
        .args([
            "new",
            "release",
            "jpx",
            "1.2.0",
            "--platforms",
            "LINUX_X64,MAC_ARM64",
        ])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("wrote requests/jpx/"))
        .stdout(predicate::str::ends_with("-release-1.2.0.yaml\n"))
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(output).unwrap();
    let path = stdout.trim().strip_prefix("wrote ").unwrap();
    let text = std::fs::read_to_string(repo.path().join(path)).unwrap();
    assert!(text.contains("LINUX_X64: REPLACE_WITH_LINUX_X64_URL"));
    assert!(text.contains("MAC_ARM64: REPLACE_WITH_MAC_ARM64_URL"));
}

#[test]
fn new_register_writes_a_registration() {
    let repo = TempRepo::new();
    Command::new(assert_cmd::cargo::cargo_bin!("contrib"))
        .current_dir(repo.path())
        .args(["new", "register", "jpx"])
        .assert()
        .success()
        .stdout(predicate::str::ends_with("-register-jpx.yaml\n"));
}
