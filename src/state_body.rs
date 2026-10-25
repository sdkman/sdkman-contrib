use serde::Serialize;

use crate::request::{Registration, Release};
use crate::verify::RowCheck;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VersionBody {
    pub candidate: String,
    pub version: String,
    pub platform: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub md5sum: Option<String>,
    pub sha256sum: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha512sum: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CandidateBody {
    pub candidate: String,
    pub name: String,
    pub description: String,
    pub website_url: String,
}

pub fn version_body(release: &Release, row: &RowCheck) -> Option<VersionBody> {
    let digests = row.digests.as_ref()?;
    let declared = release
        .versions
        .iter()
        .find(|entry| entry.version == row.version)?
        .platforms
        .get(&row.platform)?;
    let is_default = release.default.as_deref() == Some(row.version.as_str());
    Some(VersionBody {
        candidate: release.candidate.clone(),
        version: row.version.clone(),
        platform: row.platform.clone(),
        url: row.url.clone(),
        md5sum: declared.md5.as_ref().map(|_| digests.md5.clone()),
        sha256sum: digests.sha256.clone(),
        sha512sum: declared.sha512.as_ref().map(|_| digests.sha512.clone()),
        tags: is_default.then(|| vec!["stable".to_owned()]),
    })
}

pub fn candidate_body(registration: &Registration) -> CandidateBody {
    CandidateBody {
        candidate: registration.candidate.clone(),
        name: registration.name.clone(),
        description: registration.description.clone(),
        website_url: registration.website.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;
    use crate::digest::Digests;
    use crate::request::{Archive, ReleaseVersion};
    use serde_json::{Value, json};

    fn archive(md5: Option<&str>, sha256: Option<&str>, sha512: Option<&str>) -> Archive {
        Archive {
            url: "https://dl.example/jpx-1.0.0.tar.gz".to_owned(),
            md5: md5.map(str::to_owned),
            sha256: sha256.map(str::to_owned),
            sha512: sha512.map(str::to_owned),
        }
    }

    fn release(archive: Archive, default: Option<&str>) -> Release {
        Release {
            candidate: "jpx".to_owned(),
            versions: ["1.0.0", "1.1.0"]
                .into_iter()
                .map(|version| ReleaseVersion {
                    version: version.to_owned(),
                    platforms: BTreeMap::from([("LINUX_X64".to_owned(), archive.clone())]),
                })
                .collect(),
            default: default.map(str::to_owned),
        }
    }

    fn row(version: &str, digests: Option<Digests>) -> RowCheck {
        RowCheck {
            version: version.to_owned(),
            platform: "LINUX_X64".to_owned(),
            url: "https://dl.example/jpx-1.0.0.tar.gz".to_owned(),
            digests,
            checksums: Vec::new(),
            failures: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn digests() -> Digests {
        Digests {
            md5: "aa".to_owned(),
            sha256: "bb".to_owned(),
            sha512: "cc".to_owned(),
        }
    }

    fn keys(value: &Value) -> BTreeSet<&str> {
        value
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect()
    }

    fn body(release: &Release, row: &RowCheck) -> Value {
        serde_json::to_value(version_body(release, row).expect("body")).expect("json")
    }

    #[test]
    fn undeclared_checksums_post_only_the_computed_sha256() {
        let value = body(
            &release(archive(None, None, None), None),
            &row("1.0.0", Some(digests())),
        );
        assert_eq!(
            keys(&value),
            BTreeSet::from(["candidate", "version", "platform", "url", "sha256sum"])
        );
        assert_eq!(value["sha256sum"], "bb");
    }

    #[test]
    fn declared_md5_and_sha512_post_the_computed_values() {
        let value = body(
            &release(archive(Some("AA"), None, Some("CC")), None),
            &row("1.0.0", Some(digests())),
        );
        assert_eq!(
            (&value["md5sum"], &value["sha256sum"], &value["sha512sum"]),
            (&json!("aa"), &json!("bb"), &json!("cc"))
        );
    }

    #[test]
    fn declared_sha256_alone_adds_no_other_checksum() {
        let value = body(
            &release(archive(None, Some("bb"), None), None),
            &row("1.0.0", Some(digests())),
        );
        assert!(!keys(&value).contains("md5sum") && !keys(&value).contains("sha512sum"));
    }

    #[test]
    fn default_version_row_is_tagged_stable() {
        let value = body(
            &release(archive(None, None, None), Some("1.0.0")),
            &row("1.0.0", Some(digests())),
        );
        assert_eq!(value["tags"], json!(["stable"]));
    }

    #[test]
    fn non_default_row_has_no_tags_key() {
        let value = body(
            &release(archive(None, None, None), Some("1.0.0")),
            &row("1.1.0", Some(digests())),
        );
        assert!(!keys(&value).contains("tags"));
    }

    #[test]
    fn version_body_never_sets_visible_or_distribution() {
        let value = body(
            &release(archive(Some("aa"), Some("bb"), Some("cc")), Some("1.0.0")),
            &row("1.0.0", Some(digests())),
        );
        assert!(!keys(&value).contains("visible") && !keys(&value).contains("distribution"));
    }

    #[test]
    fn row_without_digests_has_no_body() {
        let release = release(archive(None, None, None), None);
        assert!(version_body(&release, &row("1.0.0", None)).is_none());
    }

    #[test]
    fn candidate_body_uses_state_key_names() {
        let value = serde_json::to_value(candidate_body(&Registration {
            candidate: "jpx".to_owned(),
            name: "JPX".to_owned(),
            description: "A tool".to_owned(),
            website: "https://jpx.dev/".to_owned(),
        }))
        .expect("json");
        assert_eq!(
            value,
            json!({
                "candidate": "jpx",
                "name": "JPX",
                "description": "A tool",
                "website_url": "https://jpx.dev/",
            })
        );
    }
}
