mod support;

use std::collections::BTreeMap;
use std::path::PathBuf;

use sdkman_contrib::request::{Archive, Release, ReleaseVersion};
use sdkman_contrib::verify::verify_release;
use support::archives::write_tar_gz;
use support::fakes::FakeArchives;
use tempfile::TempDir;

const GOOD_URL: &str = "https://dl.example/jpx-1.0.0.tar.gz";
const GOOD_ENTRIES: [(&str, u32); 3] = [
    ("jpx-1.0.0/", 0o755),
    ("jpx-1.0.0/bin/", 0o755),
    ("jpx-1.0.0/bin/jpx", 0o755),
];

fn archive(url: &str) -> Archive {
    Archive {
        url: url.to_owned(),
        md5: None,
        sha256: None,
        sha512: None,
    }
}

fn release(versions: Vec<(&str, Vec<(&str, Archive)>)>) -> Release {
    Release {
        candidate: "jpx".to_owned(),
        versions: versions
            .into_iter()
            .map(|(version, platforms)| ReleaseVersion {
                version: version.to_owned(),
                platforms: platforms
                    .into_iter()
                    .map(|(platform, archive)| (platform.to_owned(), archive))
                    .collect::<BTreeMap<_, _>>(),
            })
            .collect(),
        default: None,
    }
}

fn tar_gz(dir: &TempDir, name: &str, entries: &[(&str, u32)]) -> PathBuf {
    let path = dir.path().join(name);
    write_tar_gz(&path, entries);
    path
}

#[test]
fn good_archive_passes_with_digests() {
    let dir = TempDir::new().unwrap();
    let fakes = FakeArchives::default().serving(GOOD_URL, &tar_gz(&dir, "a.tar.gz", &GOOD_ENTRIES));
    let rows = verify_release(
        &release(vec![("1.0.0", vec![("LINUX_X64", archive(GOOD_URL))])]),
        &fakes,
    );
    assert_eq!(rows.len(), 1);
    assert!(rows[0].passed());
    assert!(rows[0].failures.is_empty() && rows[0].warnings.is_empty());
    assert_eq!(rows[0].digests.as_ref().unwrap().sha256.len(), 64);
}

#[test]
fn declared_sha512_mismatch_fails() {
    let dir = TempDir::new().unwrap();
    let fakes = FakeArchives::default().serving(GOOD_URL, &tar_gz(&dir, "a.tar.gz", &GOOD_ENTRIES));
    let declared = Archive {
        sha512: Some("0".repeat(128)),
        ..archive(GOOD_URL)
    };
    let rows = verify_release(
        &release(vec![("1.0.0", vec![("LINUX_X64", declared)])]),
        &fakes,
    );
    assert!(!rows[0].passed());
    assert_eq!(
        rows[0].failures,
        vec!["declared sha512 does not match the download"]
    );
    assert!(!rows[0].checksums[0].matched);
    assert!(rows[0].digests.is_some());
}

#[test]
fn matching_declared_checksum_passes() {
    let dir = TempDir::new().unwrap();
    let fakes = FakeArchives::default().serving(GOOD_URL, &tar_gz(&dir, "a.tar.gz", &GOOD_ENTRIES));
    let first = verify_release(
        &release(vec![("1.0.0", vec![("LINUX_X64", archive(GOOD_URL))])]),
        &fakes,
    );
    let declared = Archive {
        sha256: Some(first[0].digests.as_ref().unwrap().sha256.to_uppercase()),
        ..archive(GOOD_URL)
    };
    let rows = verify_release(
        &release(vec![("1.0.0", vec![("LINUX_X64", declared)])]),
        &fakes,
    );
    assert!(rows[0].passed());
    assert!(rows[0].checksums[0].matched);
}

#[test]
fn failed_url_skips_download() {
    let fakes = FakeArchives::default();
    let rows = verify_release(
        &release(vec![(
            "1.0.0",
            vec![("LINUX_X64", archive("http://dl.example/jpx-1.0.0.tar.gz"))],
        )]),
        &fakes,
    );
    assert_eq!(fakes.fetch_count(), 0);
    assert_eq!(rows[0].failures, vec!["URL must use https"]);
    assert!(rows[0].digests.is_none());
}

#[test]
fn download_failure_fails_the_row() {
    let rows = verify_release(
        &release(vec![("1.0.0", vec![("LINUX_X64", archive(GOOD_URL))])]),
        &FakeArchives::default(),
    );
    assert_eq!(rows[0].failures, vec!["URL answered 404, expected 200"]);
    assert!(rows[0].digests.is_none());
}

#[test]
fn layout_failure_and_warning_are_reported() {
    let dir = TempDir::new().unwrap();
    let entries = [("jpx/", 0o755), ("jpx/lib/", 0o755), ("jpx/lib/x", 0o644)];
    let fakes = FakeArchives::default().serving(GOOD_URL, &tar_gz(&dir, "a.tar.gz", &entries));
    let rows = verify_release(
        &release(vec![("1.0.0", vec![("LINUX_X64", archive(GOOD_URL))])]),
        &fakes,
    );
    assert!(!rows[0].passed());
    assert_eq!(rows[0].failures, vec!["no bin/ under jpx/"]);
    assert_eq!(
        rows[0].warnings,
        vec!["base directory is 'jpx' (expected 'jpx-1.0.0')"]
    );
}

#[test]
fn rows_follow_version_then_platform_order() {
    let rows = verify_release(
        &release(vec![
            (
                "2.0.0",
                vec![
                    ("MAC_ARM64", archive("https://dl.example/a.tar.gz")),
                    ("LINUX_X64", archive("https://dl.example/b.tar.gz")),
                ],
            ),
            (
                "1.0.0",
                vec![("WINDOWS_X64", archive("https://dl.example/c.tar.gz"))],
            ),
        ]),
        &FakeArchives::default(),
    );
    let order: Vec<_> = rows
        .iter()
        .map(|row| (row.version.as_str(), row.platform.as_str()))
        .collect();
    assert_eq!(
        order,
        vec![
            ("2.0.0", "LINUX_X64"),
            ("2.0.0", "MAC_ARM64"),
            ("1.0.0", "WINDOWS_X64")
        ]
    );
}
