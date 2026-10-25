use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;

pub const LISTING_FILE: &str = "contrib-candidates.txt";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateListing {
    candidates: BTreeSet<String>,
}

impl CandidateListing {
    pub fn load(repo_root: &Path) -> io::Result<Self> {
        fs::read_to_string(repo_root.join(LISTING_FILE)).map(|text| Self::parse(&text))
    }

    pub fn parse(text: &str) -> Self {
        let candidates = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(str::to_owned)
            .collect();
        Self { candidates }
    }

    pub fn refusal(&self, candidate: &str) -> Option<String> {
        if candidate == "java" {
            Some("java is refused: DISCO owns java".to_owned())
        } else if !self.candidates.contains(candidate) {
            Some(format!("{candidate} is not listed in {LISTING_FILE}"))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_blank_and_comment_lines() {
        let listing = CandidateListing::parse("# open\n\n  jpx  \n  # indented comment\ntomcat\n");
        assert_eq!(listing.refusal("jpx"), None);
        assert_eq!(listing.refusal("tomcat"), None);
        assert_eq!(
            listing.refusal("# open"),
            Some("# open is not listed in contrib-candidates.txt".to_owned())
        );
    }

    #[test]
    fn refuses_unlisted_candidate() {
        let listing = CandidateListing::parse("jpx\n");
        assert_eq!(
            listing.refusal("tomcat"),
            Some("tomcat is not listed in contrib-candidates.txt".to_owned())
        );
    }

    #[test]
    fn refuses_java_even_when_listed() {
        let listing = CandidateListing::parse("java\n");
        assert_eq!(
            listing.refusal("java"),
            Some("java is refused: DISCO owns java".to_owned())
        );
    }

    #[test]
    fn refuses_java_when_unlisted_with_the_java_message() {
        let listing = CandidateListing::parse("");
        assert_eq!(
            listing.refusal("java"),
            Some("java is refused: DISCO owns java".to_owned())
        );
    }

    #[test]
    fn load_reads_the_listing_file_from_the_root() {
        let missing = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        assert!(CandidateListing::load(&missing).is_err());
        assert!(CandidateListing::load(Path::new(env!("CARGO_MANIFEST_DIR"))).is_ok());
    }

    #[test]
    fn committed_listing_never_lists_java() {
        let listing = CandidateListing::load(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        assert!(!listing.candidates.contains("java"));
    }

    #[test]
    fn committed_listing_never_lists_contrib() {
        let listing = CandidateListing::load(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        assert!(!listing.candidates.contains("contrib"));
    }
}
