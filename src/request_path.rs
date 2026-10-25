use std::cmp::Ordering;
use std::fmt;
use std::path::PathBuf;

use chrono::{DateTime, NaiveDateTime, Timelike, Utc};

const TIMESTAMP_FORMAT: &str = "%Y%m%dT%H%M%SZ";
const TIMESTAMP_LEN: usize = 16;
const EXTENSION: &str = ".yaml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamingError {
    message: String,
}

impl NamingError {
    fn new(detail: impl fmt::Display) -> Self {
        Self {
            message: format!(
                "{detail}; expected requests/<candidate>/<timestamp>-<kind>-<slug>.yaml"
            ),
        }
    }
}

impl fmt::Display for NamingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for NamingError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Register,
    Release,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Register => "register",
            Kind::Release => "release",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(NaiveDateTime);

impl Timestamp {
    pub fn parse(text: &str) -> Result<Self, NamingError> {
        let bytes = text.as_bytes();
        let well_shaped = bytes.len() == TIMESTAMP_LEN
            && bytes.iter().enumerate().all(|(i, b)| match i {
                8 => *b == b'T',
                15 => *b == b'Z',
                _ => b.is_ascii_digit(),
            });
        if !well_shaped {
            return Err(NamingError::new(format!(
                "timestamp `{text}` is not YYYYMMDDTHHMMSSZ"
            )));
        }
        NaiveDateTime::parse_from_str(text, TIMESTAMP_FORMAT)
            .ok()
            .filter(|at| at.nanosecond() < 1_000_000_000)
            .map(Self)
            .ok_or_else(|| {
                NamingError::new(format!("timestamp `{text}` is not a real date and time"))
            })
    }

    pub fn from_utc(at: DateTime<Utc>) -> Self {
        Self(at.naive_utc().with_nanosecond(0).expect("zero nanoseconds"))
    }

    pub fn at(self) -> NaiveDateTime {
        self.0
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.format(TIMESTAMP_FORMAT))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestPath {
    candidate: String,
    timestamp: Timestamp,
    kind: Kind,
    slug: String,
}

impl RequestPath {
    pub fn parse(path: &str) -> Result<Self, NamingError> {
        let segments: Vec<&str> = path.split('/').collect();
        let [root, candidate, file_name] = segments[..] else {
            return Err(NamingError::new(format!(
                "`{path}` is not three segments below the repository root"
            )));
        };
        if root != "requests" || candidate.is_empty() {
            return Err(NamingError::new(format!(
                "`{path}` is not below requests/<candidate>/"
            )));
        }
        let stem = file_name.strip_suffix(EXTENSION).ok_or_else(|| {
            NamingError::new(format!("`{file_name}` does not end in {EXTENSION}"))
        })?;
        let (timestamp_text, rest) = stem
            .split_at_checked(TIMESTAMP_LEN)
            .ok_or_else(|| NamingError::new(format!("`{file_name}` is too short")))?;
        let timestamp = Timestamp::parse(timestamp_text)?;
        let rest = rest.strip_prefix('-').ok_or_else(|| {
            NamingError::new(format!("`{file_name}` has no `-` after the timestamp"))
        })?;
        let (kind, slug) = if let Some(slug) = rest.strip_prefix("register-") {
            (Kind::Register, slug)
        } else if let Some(slug) = rest.strip_prefix("release-") {
            (Kind::Release, slug)
        } else {
            return Err(NamingError::new(format!(
                "`{file_name}` kind is not `register` or `release`"
            )));
        };
        if slug.is_empty() || !slug.chars().all(is_slug_char) {
            return Err(NamingError::new(format!(
                "slug `{slug}` is not lowercase letters, digits, `.` and `-`"
            )));
        }
        Ok(Self {
            candidate: candidate.to_string(),
            timestamp,
            kind,
            slug: slug.to_string(),
        })
    }

    pub fn candidate(&self) -> &str {
        &self.candidate
    }

    pub fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn slug(&self) -> &str {
        &self.slug
    }

    pub fn file_name(&self) -> String {
        format!(
            "{}-{}-{}{EXTENSION}",
            self.timestamp,
            self.kind.as_str(),
            self.slug
        )
    }

    pub fn repo_path(&self) -> PathBuf {
        self.below("requests")
    }

    pub fn receipt_path(&self) -> PathBuf {
        self.below(".applied")
    }

    fn below(&self, root: &str) -> PathBuf {
        [root, &self.candidate, &self.file_name()].iter().collect()
    }
}

impl Ord for RequestPath {
    fn cmp(&self, other: &Self) -> Ordering {
        self.file_name()
            .cmp(&other.file_name())
            .then_with(|| self.candidate.cmp(&other.candidate))
    }
}

impl PartialOrd for RequestPath {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn is_slug_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-'
}

pub fn check_timestamp(
    timestamp: &Timestamp,
    now: DateTime<Utc>,
    latest_applied: Option<&Timestamp>,
) -> Result<(), String> {
    if timestamp.at() > now.naive_utc() {
        return Err(format!("timestamp {timestamp} is in the future"));
    }
    match latest_applied {
        Some(latest) if timestamp < latest => Err(format!(
            "timestamp {timestamp} is earlier than the latest applied request {latest}"
        )),
        _ => Ok(()),
    }
}

pub fn slugify(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if is_slug_char(c) { c } else { '-' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn parse(path: &str) -> RequestPath {
        RequestPath::parse(path).unwrap()
    }

    #[test]
    fn parses_register_path() {
        let parsed = parse("requests/jpx/20261008T140211Z-register-jpx.yaml");
        assert_eq!(parsed.candidate(), "jpx");
        assert_eq!(parsed.kind(), Kind::Register);
        assert_eq!(parsed.slug(), "jpx");
        assert_eq!(parsed.timestamp().to_string(), "20261008T140211Z");
    }

    #[test]
    fn parses_release_path_with_dotted_slug() {
        let parsed = parse("requests/jpx/20261014T094057Z-release-1.2.0.yaml");
        assert_eq!(parsed.kind(), Kind::Release);
        assert_eq!(parsed.slug(), "1.2.0");
    }

    #[test]
    fn kinds_spell_register_and_release() {
        assert_eq!(Kind::Register.as_str(), "register");
        assert_eq!(Kind::Release.as_str(), "release");
    }

    #[test]
    fn rejects_flat_path() {
        let error = RequestPath::parse("requests/jpx/1.2.0.yaml").unwrap_err();
        assert!(error.to_string().contains("<timestamp>-<kind>-<slug>.yaml"));
        assert!(RequestPath::parse("requests/20261008T140211Z-register-jpx.yaml").is_err());
    }

    #[test]
    fn rejects_short_timestamp() {
        assert!(Timestamp::parse("2026108T140211Z").is_err());
        let error =
            RequestPath::parse("requests/jpx/2026108T140211Z-register-jpx.yaml").unwrap_err();
        assert!(error.to_string().contains("<timestamp>-<kind>-<slug>.yaml"));
    }

    #[test]
    fn rejects_impossible_dates() {
        assert!(Timestamp::parse("20261308T140211Z").is_err());
        assert!(Timestamp::parse("20260230T140211Z").is_err());
        assert!(Timestamp::parse("20261008T250211Z").is_err());
        assert!(Timestamp::parse("20261008T235960Z").is_err());
    }

    #[test]
    fn rejects_malformed_timestamp_shape() {
        assert!(Timestamp::parse("20261008 140211Z").is_err());
        assert!(Timestamp::parse("20261008T140211").is_err());
        assert!(Timestamp::parse("2026-10-08T14:02:11Z").is_err());
    }

    #[test]
    fn rejects_unknown_kind() {
        assert!(RequestPath::parse("requests/jpx/20261008T140211Z-delete-jpx.yaml").is_err());
    }

    #[test]
    fn rejects_bad_slug() {
        assert!(RequestPath::parse("requests/jpx/20261008T140211Z-release-.yaml").is_err());
        assert!(RequestPath::parse("requests/jpx/20261008T140211Z-release-1.2.0_rc.yaml").is_err());
        assert!(RequestPath::parse("requests/jpx/20261008T140211Z-release-V1.yaml").is_err());
    }

    #[test]
    fn rejects_wrong_root_depth_and_extension() {
        assert!(RequestPath::parse("other/jpx/20261008T140211Z-register-jpx.yaml").is_err());
        assert!(RequestPath::parse("requests/jpx/x/20261008T140211Z-register-jpx.yaml").is_err());
        assert!(RequestPath::parse("requests//20261008T140211Z-register-jpx.yaml").is_err());
        assert!(RequestPath::parse("requests/jpx/20261008T140211Z-register-jpx.yml").is_err());
    }

    #[test]
    fn rejects_non_ascii_without_panicking() {
        assert!(RequestPath::parse("requests/jpx/2026100éT140211Z-register-jpx.yaml").is_err());
    }

    #[test]
    fn maps_to_request_and_receipt_paths() {
        let parsed = parse("requests/tomcat/20261009T081500Z-release-11.0.15.yaml");
        assert_eq!(
            parsed.repo_path(),
            Path::new("requests/tomcat/20261009T081500Z-release-11.0.15.yaml")
        );
        assert_eq!(
            parsed.receipt_path(),
            Path::new(".applied/tomcat/20261009T081500Z-release-11.0.15.yaml")
        );
    }

    #[test]
    fn orders_by_file_name_across_candidates() {
        let later = parse("requests/aaa/20261009T081500Z-release-1.yaml");
        let earlier = parse("requests/zzz/20261008T140211Z-register-zzz.yaml");
        assert!(earlier < later);
    }

    #[test]
    fn orders_equal_file_names_by_candidate() {
        let first = parse("requests/aaa/20261009T081500Z-release-1.yaml");
        let second = parse("requests/bbb/20261009T081500Z-release-1.yaml");
        assert!(first < second);
    }

    #[test]
    fn slugify_lowercases_and_replaces_other_characters() {
        assert_eq!(slugify("1.2.0-RC_1+build"), "1.2.0-rc-1-build");
        assert_eq!(slugify("JPX"), "jpx");
    }

    fn now() -> DateTime<Utc> {
        Timestamp::parse("20261010T120000Z").unwrap().at().and_utc()
    }

    fn timestamp(text: &str) -> Timestamp {
        Timestamp::parse(text).unwrap()
    }

    #[test]
    fn rejects_future_timestamp() {
        let error = check_timestamp(&timestamp("20261010T120001Z"), now(), None).unwrap_err();
        assert_eq!(error, "timestamp 20261010T120001Z is in the future");
    }

    #[test]
    fn rejects_backdated_timestamp() {
        let latest = timestamp("20261009T081500Z");
        let error =
            check_timestamp(&timestamp("20261009T081459Z"), now(), Some(&latest)).unwrap_err();
        assert_eq!(
            error,
            "timestamp 20261009T081459Z is earlier than the latest applied request 20261009T081500Z"
        );
    }

    #[test]
    fn accepts_equal_timestamp() {
        let latest = timestamp("20261009T081500Z");
        assert!(check_timestamp(&latest, now(), Some(&latest)).is_ok());
    }

    #[test]
    fn accepts_timestamp_equal_to_now_and_later_than_latest() {
        let latest = timestamp("20261009T081500Z");
        assert!(check_timestamp(&timestamp("20261010T120000Z"), now(), Some(&latest)).is_ok());
        assert!(check_timestamp(&timestamp("20261010T120000Z"), now(), None).is_ok());
    }
}
