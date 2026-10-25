mod support;

use std::collections::BTreeMap;
use std::path::Path;

use chrono::{TimeZone, Utc};
use sdkman_contrib::apply::{ApplyError, RunInfo, apply_queue, apply_request, void_request};
use sdkman_contrib::git::Git;
use sdkman_contrib::receipt::{Receipt, ReceiptRow, StableTag};
use sdkman_contrib::request::{Archive, Registration, Release, ReleaseVersion, Request};
use sdkman_contrib::request_path::RequestPath;
use sdkman_contrib::state_api::WriteError;
use sha2::{Digest, Sha256};
use support::archives::tar_gz;
use support::fakes::{FakeArchives, FakeWriter};
use support::fixtures::GOOD_ENTRIES;
use support::repo::TempRepo;
use tempfile::TempDir;

const COMMIT: &str = "9e07d13";

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
    assert_eq!(posted.description, "A tool.");
    assert_eq!(posted.website_url, "https://jpx.dev/");
    let register = receipt.register.unwrap();
    assert_eq!(register.name, "JPX");
    assert_eq!(register.description, "A tool.");
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
fn run_info_with_a_missing_github_var_is_local() {
    let now = Utc.with_ymd_and_hms(2026, 10, 14, 9, 40, 57).unwrap();
    let vars = BTreeMap::from([
        ("GITHUB_SERVER_URL", "https://github.com"),
        ("GITHUB_REPOSITORY", "sdkman/sdkman-contrib"),
    ]);
    let info = RunInfo::from_vars(now, |name| vars.get(name).map(|value| (*value).to_owned()));
    assert_eq!(info.run_url, "local");
}

#[test]
fn run_info_without_github_vars_is_local() {
    assert_eq!(run().run_url, "local");
}

const JPX_REGISTER: &str = "requests/jpx/20261008T140211Z-register-jpx.yaml";
const TOMCAT_RELEASE: &str = "requests/tomcat/20261009T081500Z-release-11.0.15.yaml";
const JPX_RELEASE: &str = "requests/jpx/20261008T100000Z-release-1.0.0.yaml";

const REGISTER_YAML: &str = "candidate: jpx
register:
  name: JPX
  description: A tool.
  website: https://jpx.dev/
";

fn tomcat_release_yaml() -> String {
    "candidate: tomcat
versions:
  - version: 11.0.15
    platforms:
      LINUX_X64: https://dl.example/tomcat-11.0.15.tar.gz
"
    .to_owned()
}

fn jpx_release_yaml(versions: &[&str]) -> String {
    let rows: String = versions
        .iter()
        .map(|version| {
            format!(
                "  - version: {version}\n    platforms:\n      LINUX_X64: {}\n",
                url(version)
            )
        })
        .collect();
    format!("candidate: jpx\nversions:\n{rows}")
}

fn queue_repo(requests: &[(&str, String)]) -> (TempRepo, Vec<String>) {
    let repo = TempRepo::with_remote();
    repo.write("contrib-candidates.txt", "jpx\ntomcat\n");
    let commits = requests
        .iter()
        .map(|(path, text)| {
            repo.write(path, text);
            repo.commit_all(&format!("add {path}"))
        })
        .collect();
    (repo, commits)
}

fn tomcat_archives(dir: &TempDir) -> FakeArchives {
    FakeArchives::default().serving(
        "https://dl.example/tomcat-11.0.15.tar.gz",
        &tar_gz(
            dir,
            "tomcat.tar.gz",
            &[
                ("tomcat-11.0.15/", 0o755),
                ("tomcat-11.0.15/bin/", 0o755),
                ("tomcat-11.0.15/bin/catalina.sh", 0o755),
            ],
        ),
    )
}

fn read_receipt(repo: &TempRepo, request: &str) -> Receipt {
    let text = std::fs::read_to_string(repo.path().join(receipt_path(request))).unwrap();
    serde_norway::from_str(&text).unwrap()
}

fn has_receipt(repo: &TempRepo, request: &str) -> bool {
    repo.path().join(receipt_path(request)).exists()
}

fn receipt_path(request: &str) -> String {
    RequestPath::parse(request).unwrap().receipt_path()
}

#[test]
fn applies_in_file_name_order_across_candidates() {
    let dir = TempDir::new().unwrap();
    let (repo, commits) = queue_repo(&[
        (TOMCAT_RELEASE, tomcat_release_yaml()),
        (JPX_REGISTER, REGISTER_YAML.to_owned()),
    ]);
    let writer = FakeWriter::default();

    let applied = apply_queue(
        repo.path(),
        &writer,
        &tomcat_archives(&dir),
        &Git::new(repo.path()),
        &run(),
    )
    .unwrap();

    assert_eq!(applied, vec![JPX_REGISTER, TOMCAT_RELEASE]);
    assert_eq!(writer.candidates().len(), 1);
    assert_eq!(writer.rows().len(), 1);
    assert!(commits[1].starts_with(&read_receipt(&repo, JPX_REGISTER).commit));
    assert!(commits[0].starts_with(&read_receipt(&repo, TOMCAT_RELEASE).commit));
    assert!(repo.remote_log().starts_with(&format!(
        "chore(apply): record {TOMCAT_RELEASE}\nchore(apply): record {JPX_REGISTER}\n"
    )));
}

#[test]
fn failure_blocks_later_requests() {
    let dir = TempDir::new().unwrap();
    let (repo, _) = queue_repo(&[
        (JPX_RELEASE, jpx_release_yaml(&["1.0.0"])),
        (TOMCAT_RELEASE, tomcat_release_yaml()),
    ]);
    let writer = FakeWriter::default();
    let git = Git::new(repo.path());

    let error =
        apply_queue(repo.path(), &writer, &tomcat_archives(&dir), &git, &run()).unwrap_err();

    assert!(error.to_string().starts_with(JPX_RELEASE), "{error}");
    assert_eq!(writer.write_attempts(), 0);
    assert!(!has_receipt(&repo, JPX_RELEASE));
    assert!(!has_receipt(&repo, TOMCAT_RELEASE));

    let archives = good_archives(&dir, &["1.0.0"]);
    let archives = archives.serving(
        "https://dl.example/tomcat-11.0.15.tar.gz",
        &dir.path().join("tomcat.tar.gz"),
    );
    let applied = apply_queue(repo.path(), &writer, &archives, &git, &run()).unwrap();
    assert_eq!(applied, vec![JPX_RELEASE, TOMCAT_RELEASE]);
}

#[test]
fn half_applied_release_reapplies_once() {
    let dir = TempDir::new().unwrap();
    let (repo, _) = queue_repo(&[(JPX_RELEASE, jpx_release_yaml(&["1.0.0", "1.1.0"]))]);
    let archives = good_archives(&dir, &["1.0.0", "1.1.0"]);
    let writer = FakeWriter::failing_on("1.1.0", "LINUX_X64");
    let git = Git::new(repo.path());

    apply_queue(repo.path(), &writer, &archives, &git, &run()).unwrap_err();
    assert_eq!(writer.rows().len(), 1);
    assert!(!has_receipt(&repo, JPX_RELEASE));

    writer.stop_failing();
    let applied = apply_queue(repo.path(), &writer, &archives, &git, &run()).unwrap();

    assert_eq!(applied, vec![JPX_RELEASE]);
    assert_eq!(writer.rows().len(), 2);
    assert_eq!(read_receipt(&repo, JPX_RELEASE).rows.unwrap().len(), 2);
    assert_eq!(repo.remote_log().matches("chore(apply): record").count(), 1);
}

#[test]
fn backdated_pending_request_fails_apply() {
    let dir = TempDir::new().unwrap();
    let backdated = "requests/jpx/20261001T000000Z-release-1.0.0.yaml";
    let (repo, _) = queue_repo(&[(backdated, jpx_release_yaml(&["1.0.0"]))]);
    repo.write(
        ".applied/jpx/20261014T094057Z-register-jpx.yaml",
        "applied_at: 2026-10-14T09:40:57Z\ncommit: 9e07d13\nrun: local\n",
    );
    repo.commit_all("receipt");
    let writer = FakeWriter::default();

    let error = apply_queue(
        repo.path(),
        &writer,
        &good_archives(&dir, &["1.0.0"]),
        &Git::new(repo.path()),
        &run(),
    )
    .unwrap_err();

    assert!(error.to_string().starts_with(backdated), "{error}");
    assert!(
        error
            .to_string()
            .contains("is earlier than the latest applied request"),
        "{error}"
    );
    assert_eq!(writer.write_attempts(), 0);
    assert!(!has_receipt(&repo, backdated));
}

#[test]
fn void_unblocks_the_queue() {
    let dir = TempDir::new().unwrap();
    let (repo, commits) = queue_repo(&[
        (JPX_RELEASE, jpx_release_yaml(&["1.0.0"])),
        (TOMCAT_RELEASE, tomcat_release_yaml()),
    ]);
    let writer = FakeWriter::default();
    let git = Git::new(repo.path());
    let archives = tomcat_archives(&dir);
    apply_queue(repo.path(), &writer, &archives, &git, &run()).unwrap_err();

    void_request(repo.path(), &git, &run(), JPX_RELEASE, "vendor withdrew it").unwrap();
    let applied = apply_queue(repo.path(), &writer, &archives, &git, &run()).unwrap();

    let receipt = read_receipt(&repo, JPX_RELEASE);
    assert_eq!(receipt.voided.as_deref(), Some("vendor withdrew it"));
    assert!(receipt.rows.is_none() && receipt.tag.is_none() && receipt.register.is_none());
    assert!(commits[0].starts_with(&receipt.commit));
    assert_eq!(applied, vec![TOMCAT_RELEASE]);
    assert_eq!(writer.rows().len(), 1);
    assert!(
        repo.remote_log()
            .contains(&format!("chore(apply): void {JPX_RELEASE}"))
    );
}

#[test]
fn void_refuses_applied_request() {
    let dir = TempDir::new().unwrap();
    let (repo, _) = queue_repo(&[(TOMCAT_RELEASE, tomcat_release_yaml())]);
    let git = Git::new(repo.path());
    let writer = FakeWriter::default();
    apply_queue(repo.path(), &writer, &tomcat_archives(&dir), &git, &run()).unwrap();
    let before = read_receipt(&repo, TOMCAT_RELEASE);

    let error = void_request(repo.path(), &git, &run(), TOMCAT_RELEASE, "oops").unwrap_err();

    assert!(error.to_string().starts_with(TOMCAT_RELEASE), "{error}");
    assert!(
        error.to_string().contains("already has a receipt"),
        "{error}"
    );
    assert_eq!(read_receipt(&repo, TOMCAT_RELEASE), before);
}

#[test]
fn void_refuses_missing_request() {
    let (repo, _) = queue_repo(&[]);

    let error = void_request(
        repo.path(),
        &Git::new(repo.path()),
        &run(),
        JPX_RELEASE,
        "oops",
    )
    .unwrap_err();

    assert!(error.to_string().contains("no such request"), "{error}");
    assert!(!has_receipt(&repo, JPX_RELEASE));
}

#[test]
fn void_of_an_uncommitted_request_is_a_failure_not_a_rejection() {
    let (repo, _) = queue_repo(&[]);
    repo.write(JPX_RELEASE, "candidate: jpx\n");

    let error = void_request(
        repo.path(),
        &Git::new(repo.path()),
        &run(),
        JPX_RELEASE,
        "oops",
    )
    .unwrap_err();

    let ApplyError::Request { cause, .. } = error else {
        panic!("expected a request error, got {error}");
    };
    assert!(matches!(*cause, ApplyError::Failed(_)), "{cause:?}");
    assert!(!has_receipt(&repo, JPX_RELEASE));
}
