use crate::archive::{check_url, inspect_archive};
use crate::digest::{ChecksumOutcome, Digests, compare_declared};
use crate::fetch::ArchiveSource;
use crate::request::{Archive, Release};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowCheck {
    pub version: String,
    pub platform: String,
    pub url: String,
    pub digests: Option<Digests>,
    pub checksums: Vec<ChecksumOutcome>,
    pub failures: Vec<String>,
    pub warnings: Vec<String>,
}

impl RowCheck {
    pub fn passed(&self) -> bool {
        self.failures.is_empty()
    }
}

pub fn verify_release(release: &Release, source: &dyn ArchiveSource) -> Vec<RowCheck> {
    release
        .versions
        .iter()
        .flat_map(|entry| {
            entry.platforms.iter().map(|(platform, archive)| {
                verify_row(
                    &release.candidate,
                    &entry.version,
                    platform,
                    archive,
                    source,
                )
            })
        })
        .collect()
}

fn verify_row(
    candidate: &str,
    version: &str,
    platform: &str,
    archive: &Archive,
    source: &dyn ArchiveSource,
) -> RowCheck {
    let (format, url_verdict) = check_url(&archive.url, candidate, version);
    let mut row = RowCheck {
        version: version.to_owned(),
        platform: platform.to_owned(),
        url: archive.url.clone(),
        digests: None,
        checksums: Vec::new(),
        failures: url_verdict.failures,
        warnings: url_verdict.warnings,
    };
    let Some(format) = format.filter(|_| row.failures.is_empty()) else {
        return row;
    };
    match source.fetch(&archive.url) {
        Ok(download) => {
            row.checksums = compare_declared(archive, &download.digests);
            row.failures
                .extend(
                    row.checksums
                        .iter()
                        .filter(|outcome| !outcome.matched)
                        .map(|outcome| {
                            format!("declared {} does not match the download", outcome.algorithm)
                        }),
                );
            let verdict =
                inspect_archive(download.file.path(), format, candidate, version, platform);
            row.failures.extend(verdict.failures);
            row.warnings.extend(verdict.warnings);
            row.digests = Some(download.digests);
        }
        Err(error) => row.failures.push(format!("URL {error}")),
    }
    row
}
