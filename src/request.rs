use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Deserialize;
use serde::de::{self, Deserializer, MapAccess, Visitor};

use crate::request_path::{Kind, RequestPath};

const ARCHIVE_KEYS: &[&str] = &["url", "md5", "sha256", "sha512"];
pub const UNIVERSAL: &str = "UNIVERSAL";
pub const LINUX_X64: &str = "LINUX_X64";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestError {
    message: String,
}

impl RequestError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for RequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RequestError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Archive {
    pub url: String,
    pub md5: Option<String>,
    pub sha256: Option<String>,
    pub sha512: Option<String>,
}

impl<'de> Deserialize<'de> for Archive {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ArchiveVisitor)
    }
}

struct ArchiveVisitor;

impl<'de> Visitor<'de> for ArchiveVisitor {
    type Value = Archive;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a URL, or a map with `url` and any of `md5`, `sha256`, `sha512`")
    }

    fn visit_str<E: de::Error>(self, url: &str) -> Result<Archive, E> {
        Ok(Archive {
            url: url.to_string(),
            md5: None,
            sha256: None,
            sha512: None,
        })
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Archive, A::Error> {
        let mut url = None;
        let mut md5 = None;
        let mut sha256 = None;
        let mut sha512 = None;
        while let Some(key) = map.next_key::<String>()? {
            let slot = match key.as_str() {
                "url" => &mut url,
                "md5" => &mut md5,
                "sha256" => &mut sha256,
                "sha512" => &mut sha512,
                _ => return Err(de::Error::unknown_field(&key, ARCHIVE_KEYS)),
            };
            if slot.is_some() {
                return Err(de::Error::custom(format!("duplicate archive key `{key}`")));
            }
            *slot = Some(map.next_value::<String>()?);
        }
        Ok(Archive {
            url: url.ok_or_else(|| de::Error::missing_field("url"))?,
            md5,
            sha256,
            sha512,
        })
    }
}

fn unique_platforms<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, Archive>, D::Error> {
    struct PlatformsVisitor;

    impl<'de> Visitor<'de> for PlatformsVisitor {
        type Value = BTreeMap<String, Archive>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a map from platform id to an archive")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut platforms = BTreeMap::new();
            while let Some((platform, archive)) = map.next_entry::<String, Archive>()? {
                if platforms.contains_key(&platform) {
                    return Err(de::Error::custom(format!(
                        "duplicate platform `{platform}`"
                    )));
                }
                platforms.insert(platform, archive);
            }
            Ok(platforms)
        }
    }

    deserializer.deserialize_map(PlatformsVisitor)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRequest {
    candidate: String,
    register: Option<RawRegister>,
    versions: Option<Vec<RawVersion>>,
    default: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRegister {
    name: String,
    description: String,
    website: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawVersion {
    version: String,
    #[serde(deserialize_with = "unique_platforms")]
    platforms: BTreeMap<String, Archive>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    pub candidate: String,
    pub name: String,
    pub description: String,
    pub website: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseVersion {
    pub version: String,
    pub platforms: BTreeMap<String, Archive>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub candidate: String,
    pub versions: Vec<ReleaseVersion>,
    pub default: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Registration(Registration),
    Release(Release),
}

impl Request {
    pub fn candidate(&self) -> &str {
        match self {
            Request::Registration(registration) => &registration.candidate,
            Request::Release(release) => &release.candidate,
        }
    }

    pub fn kind(&self) -> Kind {
        match self {
            Request::Registration(_) => Kind::Register,
            Request::Release(_) => Kind::Release,
        }
    }

    pub fn mismatches(&self, path: &RequestPath) -> Vec<String> {
        let mut mismatches = Vec::new();
        if path.candidate() != self.candidate() {
            mismatches.push(format!(
                "directory `{}` does not match candidate `{}`",
                path.candidate(),
                self.candidate()
            ));
        }
        if path.kind() != self.kind() {
            mismatches.push(format!(
                "file name says `{}` but the content is a `{}`",
                path.kind().as_str(),
                self.kind().as_str()
            ));
        }
        mismatches
    }
}

impl Release {
    pub fn structure_errors(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.versions.is_empty() {
            errors.push("a release needs at least one version".to_string());
        }
        let mut seen = BTreeSet::new();
        for entry in &self.versions {
            let version = &entry.version;
            if !seen.insert(version.as_str()) {
                errors.push(format!("version `{version}` is repeated"));
            }
            if entry.platforms.is_empty() {
                errors.push(format!("version `{version}` has no platforms"));
            } else if entry.platforms.len() > 1 && entry.platforms.contains_key(UNIVERSAL) {
                errors.push(format!(
                    "version `{version}`: {UNIVERSAL} excludes every other platform"
                ));
            }
        }
        if let Some(default) = &self.default
            && !seen.contains(default.as_str())
        {
            errors.push(format!(
                "default {default} is not a version in this request"
            ));
        }
        errors
    }
}

pub fn parse_request(text: &str) -> Result<Request, RequestError> {
    let raw: RawRequest =
        serde_norway::from_str(text).map_err(|error| RequestError::new(error.to_string()))?;
    match (raw.register, raw.versions) {
        (Some(register), None) if raw.default.is_none() => {
            Ok(Request::Registration(Registration {
                candidate: raw.candidate,
                name: register.name,
                description: register.description.trim_end_matches('\n').to_string(),
                website: register.website,
            }))
        }
        (None, Some(versions)) => Ok(Request::Release(Release {
            candidate: raw.candidate,
            versions: versions
                .into_iter()
                .map(|raw| ReleaseVersion {
                    version: raw.version,
                    platforms: raw.platforms,
                })
                .collect(),
            default: raw.default,
        })),
        (Some(_), _) => Err(RequestError::new(
            "`register` cannot be mixed with `versions` or `default`",
        )),
        (None, None) => Err(RequestError::new(
            "a request holds either `register` or `versions`",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REGISTRATION: &str = "\
candidate: jpx
register:
  name: JPX
  description: JPX is a command-line tool.
  website: https://jpx.dev/
";

    const RELEASE: &str = "\
candidate: jpx
versions:
  - version: 1.2.0
    platforms:
      LINUX_X64: https://downloads.jpx.dev/1.2.0/jpx-1.2.0-linux-x64.zip
      MAC_ARM64:
        url: https://downloads.jpx.dev/1.2.0/jpx-1.2.0-mac-arm64.zip
        sha256: 41ab
        sha512: 7d0e
default: 1.2.0
";

    fn release(text: &str) -> Release {
        match parse_request(text).unwrap() {
            Request::Release(release) => release,
            other => panic!("expected a release, got {other:?}"),
        }
    }

    fn error(text: &str) -> String {
        parse_request(text).unwrap_err().to_string()
    }

    #[test]
    fn parses_registration() {
        let request = parse_request(REGISTRATION).unwrap();
        assert_eq!(
            request,
            Request::Registration(Registration {
                candidate: "jpx".into(),
                name: "JPX".into(),
                description: "JPX is a command-line tool.".into(),
                website: "https://jpx.dev/".into(),
            })
        );
        assert_eq!(request.candidate(), "jpx");
        assert_eq!(request.kind(), Kind::Register);
    }

    #[test]
    fn parses_release_with_url_and_keyed_archives() {
        let parsed = release(RELEASE);
        assert_eq!(parsed.default.as_deref(), Some("1.2.0"));
        let platforms = &parsed.versions[0].platforms;
        assert_eq!(
            platforms["LINUX_X64"],
            Archive {
                url: "https://downloads.jpx.dev/1.2.0/jpx-1.2.0-linux-x64.zip".into(),
                md5: None,
                sha256: None,
                sha512: None,
            }
        );
        let keyed = &platforms["MAC_ARM64"];
        assert_eq!(keyed.sha256.as_deref(), Some("41ab"));
        assert_eq!(keyed.sha512.as_deref(), Some("7d0e"));
        assert_eq!(keyed.md5, None);
        assert_eq!(parse_request(RELEASE).unwrap().kind(), Kind::Release);
    }

    #[test]
    fn keeps_exact_scalar_text() {
        let parsed = release(
            "candidate: jpx\nversions:\n  - version: 8.10\n    platforms:\n      UNIVERSAL:\n        url: https://x.dev/a.zip\n        md5: 0123456789\ndefault: 8.10\n",
        );
        assert_eq!(parsed.versions[0].version, "8.10");
        assert_eq!(parsed.default.as_deref(), Some("8.10"));
        assert_eq!(
            parsed.versions[0].platforms["UNIVERSAL"].md5.as_deref(),
            Some("0123456789")
        );
    }

    #[test]
    fn strips_trailing_line_feeds_from_block_description() {
        let text = "candidate: jpx\nregister:\n  name: JPX\n  description: |+\n    First line.\n    Second line.\n\n  website: https://jpx.dev/\n";
        let Request::Registration(registration) = parse_request(text).unwrap() else {
            panic!("expected a registration");
        };
        assert_eq!(registration.description, "First line.\nSecond line.");
    }

    #[test]
    fn rejects_unknown_top_level_field() {
        let text = format!("{REGISTRATION}tags: [stable]\n");
        assert!(error(&text).contains("tags"));
    }

    #[test]
    fn rejects_unknown_registration_field() {
        let text = REGISTRATION.replace("website:", "websit:");
        assert!(error(&text).contains("websit"));
    }

    #[test]
    fn rejects_unknown_archive_key() {
        let text = RELEASE.replace("sha512:", "sha384:");
        assert!(error(&text).contains("sha384"));
    }

    #[test]
    fn rejects_repeated_archive_key() {
        let text = RELEASE.replace("sha512: 7d0e", "sha256: 7d0e");
        assert!(error(&text).contains("sha256"));
    }

    #[test]
    fn rejects_archive_without_url() {
        let text = RELEASE.replace("url:", "md5:");
        assert!(error(&text).contains("url"));
    }

    #[test]
    fn rejects_duplicate_platform() {
        let text = RELEASE.replace(
            "      MAC_ARM64:",
            "      LINUX_X64: https://downloads.jpx.dev/other.zip\n      MAC_ARM64:",
        );
        assert!(error(&text).contains("LINUX_X64"));
    }

    #[test]
    fn rejects_repeated_top_level_key() {
        let text = format!("{RELEASE}default: 1.2.0\n");
        assert!(error(&text).contains("default"));
    }

    #[test]
    fn rejects_register_mixed_with_versions() {
        let text = format!("{REGISTRATION}{}", &RELEASE["candidate: jpx\n".len()..]);
        assert!(error(&text).contains("cannot be mixed"));
    }

    #[test]
    fn rejects_register_mixed_with_default() {
        let text = format!("{REGISTRATION}default: 1.2.0\n");
        assert!(error(&text).contains("cannot be mixed"));
    }

    #[test]
    fn rejects_neither_register_nor_versions() {
        assert!(error("candidate: jpx\n").contains("either `register` or `versions`"));
        assert!(
            error("candidate: jpx\ndefault: 1.2.0\n").contains("either `register` or `versions`")
        );
    }

    #[test]
    fn leaves_empty_versions_to_the_structure_rules() {
        assert!(
            release("candidate: jpx\nversions: []\n")
                .versions
                .is_empty()
        );
        let empty_platforms =
            release("candidate: jpx\nversions:\n  - version: 1.0\n    platforms: {}\n");
        assert!(empty_platforms.versions[0].platforms.is_empty());
    }

    fn path(text: &str) -> RequestPath {
        RequestPath::parse(text).unwrap()
    }

    const RELEASE_PATH: &str = "requests/jpx/20261008T101500Z-release-1.2.0.yaml";

    #[test]
    fn accepts_matching_path_and_content() {
        let request = parse_request(RELEASE).unwrap();
        assert!(request.mismatches(&path(RELEASE_PATH)).is_empty());
        assert!(release(RELEASE).structure_errors().is_empty());
    }

    #[test]
    fn rejects_directory_mismatch() {
        let request = parse_request(RELEASE).unwrap();
        let mismatches =
            request.mismatches(&path("requests/other/20261008T101500Z-release-1.2.0.yaml"));
        assert_eq!(mismatches.len(), 1);
        assert!(mismatches[0].contains("other") && mismatches[0].contains("jpx"));
    }

    #[test]
    fn rejects_kind_mismatch() {
        let request = parse_request(REGISTRATION).unwrap();
        let mismatches = request.mismatches(&path(RELEASE_PATH));
        assert_eq!(mismatches.len(), 1);
        assert!(mismatches[0].contains("release") && mismatches[0].contains("register"));
    }

    #[test]
    fn rejects_empty_versions_and_platforms() {
        let none = release("candidate: jpx\nversions: []\n").structure_errors();
        assert_eq!(none, vec!["a release needs at least one version"]);
        let empty = release("candidate: jpx\nversions:\n  - version: 1.0\n    platforms: {}\n")
            .structure_errors();
        assert_eq!(empty, vec!["version `1.0` has no platforms"]);
    }

    #[test]
    fn rejects_repeated_version() {
        let text = format!(
            "{}  - version: 1.2.0\n    platforms:\n      UNIVERSAL: https://x.dev/a.zip\n",
            RELEASE.replace("default: 1.2.0\n", "")
        );
        assert_eq!(
            release(&text).structure_errors(),
            vec!["version `1.2.0` is repeated"]
        );
    }

    #[test]
    fn rejects_universal_mix() {
        let text = RELEASE.replace(
            "      LINUX_X64:",
            "      UNIVERSAL: https://downloads.jpx.dev/1.2.0/jpx-1.2.0.zip\n      LINUX_X64:",
        );
        let errors = release(&text).structure_errors();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("UNIVERSAL excludes every other platform"));
    }

    #[test]
    fn accepts_lone_universal() {
        let text = "candidate: jpx\nversions:\n  - version: 1.0\n    platforms:\n      UNIVERSAL: https://x.dev/a.zip\n";
        assert!(release(text).structure_errors().is_empty());
    }

    #[test]
    fn rejects_foreign_default() {
        let text = RELEASE.replace("default: 1.2.0", "default: 9.9.9");
        assert_eq!(
            release(&text).structure_errors(),
            vec!["default 9.9.9 is not a version in this request"]
        );
    }
}
