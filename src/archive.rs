use url::Url;

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
