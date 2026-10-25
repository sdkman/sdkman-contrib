use std::fmt;
use std::fs::File;
use std::io;
use std::path::Path;

use crate::archive::{ArchiveFormat, inspect_archive};
use crate::digest::HashingWriter;
use crate::state_api::VendorState;
use crate::state_body::VersionBody;

pub struct ReleasePlatform {
    pub platform: &'static str,
    pub suffix: &'static str,
}

/// `LINUX_X64` comes last: State moves the default with its `stable` tag.
pub const RELEASE_PLATFORMS: [ReleasePlatform; 5] = [
    ReleasePlatform {
        platform: "LINUX_ARM64",
        suffix: "linux-arm64.tar.gz",
    },
    ReleasePlatform {
        platform: "MAC_X64",
        suffix: "macos-x64.tar.gz",
    },
    ReleasePlatform {
        platform: "MAC_ARM64",
        suffix: "macos-arm64.tar.gz",
    },
    ReleasePlatform {
        platform: "WINDOWS_X64",
        suffix: "windows-x64.zip",
    },
    ReleasePlatform {
        platform: "LINUX_X64",
        suffix: "linux-x64.tar.gz",
    },
];

const RELEASE_CANDIDATE: &str = "contrib";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseArchiveError {
    pub failures: Vec<String>,
}

impl fmt::Display for ReleaseArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.failures.join("\n"))
    }
}

impl std::error::Error for ReleaseArchiveError {}

pub fn release_bodies(
    dir: &Path,
    version: &str,
    url_base: &str,
) -> Result<Vec<VersionBody>, ReleaseArchiveError> {
    let mut bodies = Vec::new();
    let mut failures = Vec::new();
    for release_platform in &RELEASE_PLATFORMS {
        let file_name = format!("{RELEASE_CANDIDATE}-{version}-{}", release_platform.suffix);
        match release_body(dir, &file_name, version, url_base, release_platform) {
            Ok(body) => bodies.push(body),
            Err(reasons) => failures.extend(
                reasons
                    .into_iter()
                    .map(|reason| format!("{file_name}: {reason}")),
            ),
        }
    }
    if failures.is_empty() {
        Ok(bodies)
    } else {
        Err(ReleaseArchiveError { failures })
    }
}

fn release_body(
    dir: &Path,
    file_name: &str,
    version: &str,
    url_base: &str,
    release_platform: &ReleasePlatform,
) -> Result<VersionBody, Vec<String>> {
    let path = dir.join(file_name);
    let format = if file_name.ends_with(".zip") {
        ArchiveFormat::Zip
    } else {
        ArchiveFormat::TarGz
    };
    let sha256 = sha256_of(&path).map_err(|error| vec![format!("cannot read: {error}")])?;
    let verdict = inspect_archive(
        &path,
        format,
        RELEASE_CANDIDATE,
        version,
        release_platform.platform,
    );
    let problems: Vec<String> = verdict
        .failures
        .into_iter()
        .chain(verdict.warnings)
        .collect();
    if !problems.is_empty() {
        return Err(problems);
    }
    Ok(VersionBody {
        candidate: RELEASE_CANDIDATE.to_owned(),
        version: version.to_owned(),
        platform: release_platform.platform.to_owned(),
        url: format!("{}/{file_name}", url_base.trim_end_matches('/')),
        md5sum: None,
        sha256sum: sha256,
        sha512sum: None,
        tags: None,
    })
}

fn sha256_of(path: &Path) -> io::Result<String> {
    let mut writer = HashingWriter::new(io::sink());
    io::copy(&mut File::open(path)?, &mut writer)?;
    Ok(writer.finish().1.sha256)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishError {
    pub cause: String,
    pub rollback_failures: Vec<String>,
}

impl fmt::Display for PublishError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.cause)?;
        if !self.rollback_failures.is_empty() {
            write!(
                f,
                "; the rollback also failed: {}",
                self.rollback_failures.join("; ")
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for PublishError {}

struct ReleaseRow {
    body: VersionBody,
    previous_stable: Option<String>,
    existed: bool,
}

#[derive(Default)]
struct Progress {
    posted: usize,
    tagged: usize,
}

pub fn publish_release(
    state: &dyn VendorState,
    bodies: &[VersionBody],
) -> Result<(), PublishError> {
    let rows = read_rows(state, bodies).map_err(|cause| PublishError {
        cause,
        rollback_failures: Vec::new(),
    })?;
    let mut progress = Progress::default();
    match write_rows(state, &rows, &mut progress) {
        Ok(()) => Ok(()),
        Err(cause) => Err(PublishError {
            cause,
            rollback_failures: roll_back(state, &rows, &progress),
        }),
    }
}

fn read_rows(state: &dyn VendorState, bodies: &[VersionBody]) -> Result<Vec<ReleaseRow>, String> {
    RELEASE_PLATFORMS
        .iter()
        .map(|release_platform| {
            let platform = release_platform.platform;
            let body = bodies
                .iter()
                .find(|body| body.platform == platform)
                .ok_or_else(|| format!("no {platform} row to publish"))?;
            let previous_stable = state
                .stable_version(&body.candidate, platform)
                .map_err(|error| format!("reading the stable version of {platform}: {error}"))?;
            let existed = state
                .has_version(&body.candidate, &body.version, platform)
                .map_err(|error| format!("reading {} of {platform}: {error}", body.version))?;
            Ok(ReleaseRow {
                body: VersionBody {
                    tags: None,
                    ..body.clone()
                },
                previous_stable,
                existed,
            })
        })
        .collect()
}

fn write_rows(
    state: &dyn VendorState,
    rows: &[ReleaseRow],
    progress: &mut Progress,
) -> Result<(), String> {
    for row in rows {
        state
            .post_version(&row.body)
            .map_err(|error| format!("posting {}: {error}", row.body.platform))?;
        progress.posted += 1;
    }
    for row in rows {
        let body = &row.body;
        state
            .tag_stable(&body.candidate, &body.version, &body.platform)
            .map_err(|error| format!("tagging {}: {error}", body.platform))?;
        progress.tagged += 1;
    }
    Ok(())
}

fn roll_back(state: &dyn VendorState, rows: &[ReleaseRow], progress: &Progress) -> Vec<String> {
    let mut failures = Vec::new();
    for row in rows[..progress.tagged].iter().rev() {
        let body = &row.body;
        let restored = match row.previous_stable.as_deref() {
            Some(version) if version == body.version => Ok(()),
            Some(version) => state.tag_stable(&body.candidate, version, &body.platform),
            None => state.untag_stable(&body.candidate, &body.platform),
        };
        if let Err(error) = restored {
            failures.push(format!("restoring {}: {error}", body.platform));
        }
    }
    for row in rows[..progress.posted].iter().filter(|row| !row.existed) {
        let body = &row.body;
        if let Err(error) = state.delete_version(&body.candidate, &body.version, &body.platform) {
            failures.push(format!("deleting {}: {error}", body.platform));
        }
    }
    failures
}
