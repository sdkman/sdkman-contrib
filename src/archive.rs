use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use flate2::read::GzDecoder;
use tar::EntryType;
use url::Url;
use zip::ZipArchive;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    TarGz,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Verdict {
    pub failures: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn check_url(url: &str, candidate: &str, version: &str) -> (Option<ArchiveFormat>, Verdict) {
    let mut verdict = Verdict::default();
    let parsed = match Url::parse(url) {
        Ok(parsed) => parsed,
        Err(error) => {
            verdict.failures.push(format!("URL is not valid: {error}"));
            return (None, verdict);
        }
    };
    if parsed.scheme() != "https" {
        verdict.failures.push("URL must use https".to_owned());
    }
    if parsed.path().contains("/releases/latest/") {
        verdict
            .failures
            .push("URL must name a fixed release, not /releases/latest/".to_owned());
    }
    let file_name = parsed
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .unwrap_or("");
    let (format, stem) = if let Some(stem) = file_name.strip_suffix(".zip") {
        (Some(ArchiveFormat::Zip), stem)
    } else if let Some(stem) = file_name.strip_suffix(".tar.gz") {
        (Some(ArchiveFormat::TarGz), stem)
    } else {
        verdict
            .failures
            .push("URL must end in .zip or .tar.gz".to_owned());
        return (None, verdict);
    };
    let conventional = format!("{candidate}-{version}");
    if stem != conventional && !stem.starts_with(&format!("{conventional}-")) {
        verdict.warnings.push(format!(
            "file name '{file_name}' does not follow '{conventional}' or '{conventional}-<suffix>'"
        ));
    }
    (format, verdict)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub components: Vec<String>,
    pub is_dir: bool,
    pub executable: bool,
}

const EXECUTE_BITS: u32 = 0o111;

pub fn list_entries(path: &Path, format: ArchiveFormat) -> Result<Vec<ArchiveEntry>, String> {
    match format {
        ArchiveFormat::Zip => list_zip(path).map_err(|error| format!("not a zip archive: {error}")),
        ArchiveFormat::TarGz => {
            list_tar_gz(path).map_err(|error| format!("not a tar.gz archive: {error}"))
        }
    }
}

fn list_zip(path: &Path) -> Result<Vec<ArchiveEntry>, Box<dyn std::error::Error>> {
    let mut archive = ZipArchive::new(BufReader::new(File::open(path)?))?;
    let mut entries = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let file = archive.by_index_raw(index)?;
        let is_dir = file.is_dir();
        let executable = !is_dir
            && file
                .unix_mode()
                .is_some_and(|mode| mode & EXECUTE_BITS != 0);
        entries.push(ArchiveEntry {
            components: split_components(&file.name()?),
            is_dir,
            executable,
        });
    }
    Ok(entries)
}

fn list_tar_gz(path: &Path) -> Result<Vec<ArchiveEntry>, Box<dyn std::error::Error>> {
    let mut archive = tar::Archive::new(GzDecoder::new(BufReader::new(File::open(path)?)));
    let mut entries = Vec::new();
    for entry in archive.entries()? {
        let entry = entry?;
        let kind = entry.header().entry_type();
        if matches!(kind, EntryType::XGlobalHeader | EntryType::XHeader) {
            continue;
        }
        let carries_mode = kind.is_file() || kind.is_symlink() || kind.is_hard_link();
        let executable = carries_mode && entry.header().mode()? & EXECUTE_BITS != 0;
        entries.push(ArchiveEntry {
            components: split_components(&entry.path()?.to_string_lossy()),
            is_dir: kind.is_dir(),
            executable,
        });
    }
    Ok(entries)
}

const WINDOWS_PLATFORM: &str = "WINDOWS_X64";

pub fn check_layout(
    entries: &[ArchiveEntry],
    candidate: &str,
    version: &str,
    platform: &str,
) -> Verdict {
    let mut verdict = Verdict::default();
    let Some(base) = base_directory(entries) else {
        verdict
            .failures
            .push("does not hold exactly one base directory".to_owned());
        return verdict;
    };
    let in_base = || entries.iter().filter(|entry| entry.components.len() > 1);
    let has_bin = in_base()
        .any(|entry| entry.components[1] == "bin" && (entry.components.len() > 2 || entry.is_dir));
    if !has_bin {
        verdict.failures.push(format!("no bin/ under {base}/"));
    } else if platform != WINDOWS_PLATFORM
        && !in_base().any(|entry| {
            entry.components.len() == 3 && entry.components[1] == "bin" && entry.executable
        })
    {
        verdict
            .failures
            .push(format!("no executable file in {base}/bin/"));
    }
    let conventional = format!("{candidate}-{version}");
    if base != conventional {
        verdict.warnings.push(format!(
            "base directory is '{base}' (expected '{conventional}')"
        ));
    }
    verdict
}

fn base_directory(entries: &[ArchiveEntry]) -> Option<&str> {
    let named: Vec<&ArchiveEntry> = entries
        .iter()
        .filter(|entry| !entry.components.is_empty())
        .collect();
    let base = named.first()?.components[0].as_str();
    let one_root = named
        .iter()
        .all(|entry| entry.components[0] == base && (entry.components.len() > 1 || entry.is_dir));
    let has_content = named.iter().any(|entry| entry.components.len() > 1);
    (one_root && has_content).then_some(base)
}

pub fn inspect_archive(
    path: &Path,
    format: ArchiveFormat,
    candidate: &str,
    version: &str,
    platform: &str,
) -> Verdict {
    match list_entries(path, format) {
        Ok(entries) => check_layout(&entries, candidate, version, platform),
        Err(error) => Verdict {
            failures: vec![error],
            warnings: Vec::new(),
        },
    }
}

fn split_components(name: &str) -> Vec<String> {
    let mut components: Vec<String> = name
        .split('/')
        .filter(|component| !component.is_empty())
        .map(str::to_owned)
        .collect();
    if components.first().is_some_and(|first| first == ".") {
        components.remove(0);
    }
    components
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_conventional_zip() {
        let (format, verdict) = check_url("https://example.org/dl/jpx-1.2.0.zip", "jpx", "1.2.0");
        assert_eq!(format, Some(ArchiveFormat::Zip));
        assert_eq!(verdict, Verdict::default());
    }

    #[test]
    fn accepts_tar_gz_with_platform_suffix() {
        let (format, verdict) = check_url(
            "https://example.org/jpx-1.2.0-linux-x64.tar.gz",
            "jpx",
            "1.2.0",
        );
        assert_eq!(format, Some(ArchiveFormat::TarGz));
        assert_eq!(verdict, Verdict::default());
    }

    #[test]
    fn rejects_http_url() {
        let (format, verdict) = check_url("http://example.org/jpx-1.2.0.zip", "jpx", "1.2.0");
        assert_eq!(format, Some(ArchiveFormat::Zip));
        assert_eq!(verdict.failures, vec!["URL must use https"]);
    }

    #[test]
    fn rejects_unknown_extension() {
        let (format, verdict) = check_url("https://example.org/jpx-1.2.0.tgz", "jpx", "1.2.0");
        assert_eq!(format, None);
        assert_eq!(verdict.failures, vec!["URL must end in .zip or .tar.gz"]);
    }

    #[test]
    fn ignores_query_when_reading_the_extension() {
        let (format, verdict) = check_url(
            "https://example.org/jpx-1.2.0.zip?token=a.tar",
            "jpx",
            "1.2.0",
        );
        assert_eq!(format, Some(ArchiveFormat::Zip));
        assert_eq!(verdict, Verdict::default());
    }

    #[test]
    fn rejects_url_without_a_file_name() {
        let (format, verdict) = check_url("https://example.org/dl/", "jpx", "1.2.0");
        assert_eq!(format, None);
        assert_eq!(verdict.failures, vec!["URL must end in .zip or .tar.gz"]);
    }

    #[test]
    fn rejects_text_that_is_not_a_url() {
        let (format, verdict) = check_url("jpx-1.2.0.zip", "jpx", "1.2.0");
        assert_eq!(format, None);
        assert_eq!(verdict.failures.len(), 1);
        assert!(verdict.failures[0].starts_with("URL is not valid"));
    }

    #[test]
    fn reports_scheme_and_extension_together() {
        let (_, verdict) = check_url("ftp://example.org/jpx-1.2.0.rar", "jpx", "1.2.0");
        assert_eq!(verdict.failures.len(), 2);
    }

    #[test]
    fn rejects_latest_release_download() {
        let (format, verdict) = check_url(
            "https://github.com/jpx/jpx/releases/latest/download/jpx-1.2.0.zip",
            "jpx",
            "1.2.0",
        );
        assert_eq!(format, Some(ArchiveFormat::Zip));
        assert_eq!(
            verdict.failures,
            vec!["URL must name a fixed release, not /releases/latest/"]
        );
    }

    #[test]
    fn accepts_fixed_release_download() {
        let (format, verdict) = check_url(
            "https://github.com/jpx/jpx/releases/download/v1/jpx-1.2.0.zip",
            "jpx",
            "1.2.0",
        );
        assert_eq!(format, Some(ArchiveFormat::Zip));
        assert_eq!(verdict, Verdict::default());
    }

    #[test]
    fn warns_on_unconventional_name() {
        let (format, verdict) = check_url("https://example.org/tool.zip", "jpx", "1.2.0");
        assert_eq!(format, Some(ArchiveFormat::Zip));
        assert!(verdict.failures.is_empty());
        assert_eq!(
            verdict.warnings,
            vec!["file name 'tool.zip' does not follow 'jpx-1.2.0' or 'jpx-1.2.0-<suffix>'"]
        );
    }

    #[test]
    fn warns_when_version_is_only_a_prefix() {
        let (_, verdict) = check_url("https://example.org/jpx-1.2.01.zip", "jpx", "1.2.0");
        assert_eq!(verdict.warnings.len(), 1);
    }
}
