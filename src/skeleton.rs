use std::io;
use std::path::Path;

use chrono::{DateTime, Utc};

use crate::receipt::write_new;
use crate::request_path::{Kind, NamingError, RequestPath, Timestamp, slugify};

pub fn skeleton_path(
    candidate: &str,
    kind: Kind,
    slug_source: &str,
    now: DateTime<Utc>,
) -> Result<String, NamingError> {
    RequestPath::new(
        candidate,
        Timestamp::from_utc(now),
        kind,
        &slugify(slug_source),
    )
    .map(|path| path.repo_path())
}

pub fn registration_skeleton(candidate: &str) -> String {
    format!(
        "candidate: {candidate}\nregister:\n  name: REPLACE_WITH_NAME\n  description: REPLACE_WITH_DESCRIPTION\n  website: REPLACE_WITH_WEBSITE_URL\n"
    )
}

pub fn release_skeleton(candidate: &str, version: &str, platforms: &[String]) -> String {
    let rows: String = platforms
        .iter()
        .map(|platform| format!("      {platform}: REPLACE_WITH_{platform}_URL\n"))
        .collect();
    format!(
        "candidate: {candidate}\n# default: {version}\nversions:\n  - version: {version}\n    platforms:\n{rows}"
    )
}

pub fn write_skeleton(repo_root: &Path, path: &str, text: &str) -> io::Result<()> {
    write_new(repo_root, path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        Timestamp::parse("20261014T094057Z").unwrap().at().and_utc()
    }

    #[test]
    fn registration_path_slugs_the_candidate() {
        let path = skeleton_path("jpx", Kind::Register, "jpx", now()).unwrap();
        assert_eq!(path, "requests/jpx/20261014T094057Z-register-jpx.yaml");
    }

    #[test]
    fn release_path_slugs_the_version() {
        let path = skeleton_path("jpx", Kind::Release, "1.2.0+RC_1", now()).unwrap();
        assert_eq!(
            path,
            "requests/jpx/20261014T094057Z-release-1.2.0-rc-1.yaml"
        );
    }

    #[test]
    fn path_drops_subsecond_time() {
        let later = now() + chrono::Duration::milliseconds(900);
        let path = skeleton_path("jpx", Kind::Register, "jpx", later).unwrap();
        assert_eq!(path, "requests/jpx/20261014T094057Z-register-jpx.yaml");
    }

    #[test]
    fn path_refuses_an_empty_slug() {
        assert!(skeleton_path("jpx", Kind::Release, "", now()).is_err());
    }

    #[test]
    fn release_skeleton_lists_each_platform() {
        let platforms = vec!["LINUX_X64".to_owned(), "MAC_ARM64".to_owned()];
        let text = release_skeleton("jpx", "1.2.0", &platforms);
        assert!(text.contains("# default: 1.2.0\n"));
        assert!(text.contains("      LINUX_X64: REPLACE_WITH_LINUX_X64_URL\n"));
        assert!(text.contains("      MAC_ARM64: REPLACE_WITH_MAC_ARM64_URL\n"));
    }
}
