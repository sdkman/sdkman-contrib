mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use sdkman_contrib::apply::{ApplyError, RunInfo, apply_request};
use sdkman_contrib::receipt::{ReceiptRow, StableTag};
use sdkman_contrib::request::{Archive, Registration, Release, ReleaseVersion, Request};
use sdkman_contrib::state_api::WriteError;
use sha2::{Digest, Sha256};
use support::archives::write_tar_gz;
use support::fakes::{FakeArchives, FakeWriter};
use tempfile::TempDir;

const COMMIT: &str = "9e07d13";
const GOOD_ENTRIES: [(&str, u32); 3] = [
    ("jpx-1.0.0/", 0o755),
    ("jpx-1.0.0/bin/", 0o755),
    ("jpx-1.0.0/bin/jpx", 0o755),
];

fn url(version: &str) -> String {
    format!("https://dl.example/jpx-{version}.tar.gz")
}

fn archive(version: &str) -> Archive {
    Archive {
        url: url(version),
        md5: None,
        sha256: None,
        sha512: None,
    }
}

fn release(versions: Vec<(&str, Archive)>, default: Option<&str>) -> Request {
    Request::Release(Release {
        candidate: "jpx".to_owned(),
        versions: versions
            .into_iter()
            .map(|(version, archive)| ReleaseVersion {
                version: version.to_owned(),
                platforms: BTreeMap::from([("LINUX_X64".to_owned(), archive)]),
            })
            .collect(),
        default: default.map(str::to_owned),
    })
}

fn tar_gz(dir: &TempDir, name: &str, entries: &[(&str, u32)]) -> PathBuf {
    let path = dir.path().join(name);
    write_tar_gz(&path, entries);
    path
}

fn sha256_of(path: &Path) -> String {
    hex::encode(Sha256::digest(std::fs::read(path).unwrap()))
}

fn run() -> RunInfo {
    RunInfo::from_vars(
        Utc.with_ymd_and_hms(2026, 10, 14, 9, 40, 57).unwrap(),
        |_| None,
    )
}

fn good_archives(dir: &TempDir, versions: &[&str]) -> FakeArchives {
    versions
        .iter()
        .fold(FakeArchives::default(), |fakes, version| {
            fakes.serving(
                &url(version),
                &tar_gz(dir, &format!("{version}.tar.gz"), &GOOD_ENTRIES),
            )
        })
}

#[test]
fn replaced_archive_writes_nothing() {
    let dir = TempDir::new().unwrap();
    let archives = good_archives(&dir, &["1.0.0"]);
    let declared = Archive {
        sha256: Some("0".repeat(64)),
        ..archive("1.0.0")
    };
    let writer = FakeWriter::default();
    let error = apply_request(
        &release(vec![("1.0.0", declared)], None),
        &writer,
        &archives,
        COMMIT,
        &run(),
    )
    .unwrap_err();
    assert_eq!(
        error,
        ApplyError::Rejected(vec![
            "jpx 1.0.0 LINUX_X64: declared sha256 does not match the download".to_owned()
        ])
    );
    assert_eq!(writer.write_attempts(), 0);
}

#[test]
fn undeclared_archive_without_bin_writes_nothing() {
    let dir = TempDir::new().unwrap();
    let broken = tar_gz(
        &dir,
        "broken.tar.gz",
        &[("jpx-1.0.0/", 0o755), ("jpx-1.0.0/README", 0o644)],
    );
    let archives = FakeArchives::default().serving(&url("1.0.0"), &broken);
    let writer = FakeWriter::default();
    let error = apply_request(
        &release(vec![("1.0.0", archive("1.0.0"))], None),
        &writer,
        &archives,
        COMMIT,
        &run(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("no bin/ under jpx-1.0.0/"));
    assert_eq!(writer.write_attempts(), 0);
}

#[test]
fn failure_in_a_later_row_writes_nothing() {
    let dir = TempDir::new().unwrap();
    let archives = good_archives(&dir, &["1.0.0"]);
    let writer = FakeWriter::default();
    let request = release(
        vec![("1.0.0", archive("1.0.0")), ("1.1.0", archive("1.1.0"))],
        None,
    );
    let error = apply_request(&request, &writer, &archives, COMMIT, &run()).unwrap_err();
    assert!(error.to_string().contains("jpx 1.1.0 LINUX_X64"));
    assert_eq!(writer.write_attempts(), 0);
}

#[test]
fn undeclared_archive_posts_computed_sha256() {
    let dir = TempDir::new().unwrap();
    let file = tar_gz(&dir, "a.tar.gz", &GOOD_ENTRIES);
    let archives = FakeArchives::default().serving(&url("1.0.0"), &file);
    let writer = FakeWriter::default();
    let receipt = apply_request(
        &release(vec![("1.0.0", archive("1.0.0"))], None),
        &writer,
        &archives,
        COMMIT,
        &run(),
    )
    .unwrap();
    let expected = sha256_of(&file);
    let posted = &writer.rows()[&("1.0.0".to_owned(), "LINUX_X64".to_owned())];
    assert_eq!(posted.sha256sum, expected);
    assert!(posted.md5sum.is_none() && posted.sha512sum.is_none());
    assert_eq!(
        receipt.rows,
        Some(vec![ReceiptRow {
            version: "1.0.0".to_owned(),
            platform: "LINUX_X64".to_owned(),
            sha256: expected,
            md5: None,
            sha512: None,
        }])
    );
}

#[test]
fn declared_md5_and_sha512_reach_state_and_the_receipt() {
    let dir = TempDir::new().unwrap();
    let file = tar_gz(&dir, "a.tar.gz", &GOOD_ENTRIES);
    let archives = FakeArchives::default().serving(&url("1.0.0"), &file);
    let probe = sdkman_contrib::verify::verify_release(
        &match release(vec![("1.0.0", archive("1.0.0"))], None) {
            Request::Release(release) => release,
            Request::Registration(_) => unreachable!(),
        },
        &archives,
    );
    let digests = probe[0].digests.clone().unwrap();
    let declared = Archive {
        md5: Some(digests.md5.clone()),
        sha512: Some(digests.sha512.clone()),
        ..archive("1.0.0")
    };
    let writer = FakeWriter::default();
    let receipt = apply_request(
        &release(vec![("1.0.0", declared)], None),
        &writer,
        &archives,
        COMMIT,
        &run(),
    )
    .unwrap();
    let row = &receipt.rows.unwrap()[0];
    assert_eq!(row.md5.as_deref(), Some(digests.md5.as_str()));
    assert_eq!(row.sha512.as_deref(), Some(digests.sha512.as_str()));
    let posted = &writer.rows()[&("1.0.0".to_owned(), "LINUX_X64".to_owned())];
    assert_eq!(posted.md5sum.as_deref(), Some(digests.md5.as_str()));
    assert_eq!(posted.sha512sum.as_deref(), Some(digests.sha512.as_str()));
}

#[test]
fn default_rows_post_stable_tag() {
    let dir = TempDir::new().unwrap();
    let archives = good_archives(&dir, &["1.0.0", "1.1.0"]);
    let writer = FakeWriter::default();
    let receipt = apply_request(
        &release(
            vec![("1.0.0", archive("1.0.0")), ("1.1.0", archive("1.1.0"))],
            Some("1.1.0"),
        ),
        &writer,
        &archives,
        COMMIT,
        &run(),
    )
    .unwrap();
    let rows = writer.rows();
    assert_eq!(
        rows[&("1.1.0".to_owned(), "LINUX_X64".to_owned())].tags,
        Some(vec!["stable".to_owned()])
    );
    assert_eq!(
        rows[&("1.0.0".to_owned(), "LINUX_X64".to_owned())].tags,
        None
    );
    assert_eq!(
        receipt.tag,
        Some(StableTag {
            stable: "1.1.0".to_owned()
        })
    );
}

#[test]
fn refused_row_stops_the_release_and_names_it() {
    let dir = TempDir::new().unwrap();
    let archives = good_archives(&dir, &["1.0.0", "1.1.0"]);
    let writer = FakeWriter::failing_on("1.1.0", "LINUX_X64");
    let request = release(
        vec![("1.0.0", archive("1.0.0")), ("1.1.0", archive("1.1.0"))],
        None,
    );
    let error = apply_request(&request, &writer, &archives, COMMIT, &run()).unwrap_err();
    assert_eq!(
        error,
        ApplyError::Write {
            subject: "jpx 1.1.0 LINUX_X64".to_owned(),
            error: WriteError::Refused {
                status: 400,
                messages: vec!["refused on demand".to_owned()],
            },
        }
    );
    assert_eq!(writer.rows().len(), 1);
}

#[test]
fn registration_posts_candidate_and_records_metadata() {
    let writer = FakeWriter::default();
    let request = Request::Registration(Registration {
        candidate: "jpx".to_owned(),
        name: "JPX".to_owned(),
        description: "A tool.".to_owned(),
        website: "https://jpx.dev/".to_owned(),
    });
    let receipt =
        apply_request(&request, &writer, &FakeArchives::default(), COMMIT, &run()).unwrap();
    let posted = &writer.candidates()[0];
    assert_eq!(posted.website_url, "https://jpx.dev/");
    let register = receipt.register.unwrap();
    assert_eq!(register.name, "JPX");
    assert_eq!(register.website, "https://jpx.dev/");
    assert!(receipt.rows.is_none() && receipt.tag.is_none());
    assert_eq!(receipt.commit, COMMIT);
}

#[test]
fn run_info_reads_the_github_run_url() {
    let now = Utc.with_ymd_and_hms(2026, 10, 14, 9, 40, 57).unwrap();
    let vars = BTreeMap::from([
        ("GITHUB_SERVER_URL", "https://github.com"),
        ("GITHUB_REPOSITORY", "sdkman/sdkman-contrib"),
        ("GITHUB_RUN_ID", "123"),
    ]);
    let info = RunInfo::from_vars(now, |name| vars.get(name).map(|value| (*value).to_owned()));
    assert_eq!(info.applied_at, "2026-10-14T09:40:57Z");
    assert_eq!(
        info.run_url,
        "https://github.com/sdkman/sdkman-contrib/actions/runs/123"
    );
}

#[test]
fn run_info_without_github_vars_is_local() {
    assert_eq!(run().run_url, "local");
}
