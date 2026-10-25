use std::io::{self, Write};

use md5::Md5;
use sha2::{Digest, Sha256, Sha512};

use crate::request::Archive;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Digests {
    pub md5: String,
    pub sha256: String,
    pub sha512: String,
}

pub struct HashingWriter<W: Write> {
    inner: W,
    md5: Md5,
    sha256: Sha256,
    sha512: Sha512,
}

impl<W: Write> HashingWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            md5: Md5::new(),
            sha256: Sha256::new(),
            sha512: Sha512::new(),
        }
    }

    pub fn finish(self) -> (W, Digests) {
        let digests = Digests {
            md5: hex::encode(self.md5.finalize()),
            sha256: hex::encode(self.sha256.finalize()),
            sha512: hex::encode(self.sha512.finalize()),
        };
        (self.inner, digests)
    }
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buf)?;
        let accepted = &buf[..written];
        self.md5.update(accepted);
        self.sha256.update(accepted);
        self.sha512.update(accepted);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecksumOutcome {
    pub algorithm: &'static str,
    pub declared: String,
    pub matched: bool,
}

pub fn compare_declared(archive: &Archive, digests: &Digests) -> Vec<ChecksumOutcome> {
    [
        ("md5", &archive.md5, &digests.md5),
        ("sha256", &archive.sha256, &digests.sha256),
        ("sha512", &archive.sha512, &digests.sha512),
    ]
    .into_iter()
    .filter_map(|(algorithm, declared, computed)| {
        declared.as_ref().map(|declared| ChecksumOutcome {
            algorithm,
            declared: declared.clone(),
            matched: declared.eq_ignore_ascii_case(computed),
        })
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ABC_MD5: &str = "900150983cd24fb0d6963f7d28e17f72";
    const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    const ABC_SHA512: &str = "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f";

    fn digests_of_abc() -> Digests {
        let mut writer = HashingWriter::new(Vec::new());
        writer.write_all(b"abc").unwrap();
        writer.finish().1
    }

    fn archive(md5: Option<&str>, sha256: Option<&str>, sha512: Option<&str>) -> Archive {
        Archive {
            url: "https://example.com/a.zip".to_string(),
            md5: md5.map(str::to_string),
            sha256: sha256.map(str::to_string),
            sha512: sha512.map(str::to_string),
        }
    }

    #[test]
    fn computes_known_digests_of_abc() {
        assert_eq!(
            digests_of_abc(),
            Digests {
                md5: ABC_MD5.to_string(),
                sha256: ABC_SHA256.to_string(),
                sha512: ABC_SHA512.to_string(),
            }
        );
    }

    #[test]
    fn passes_bytes_through_to_the_inner_writer() {
        let mut writer = HashingWriter::new(Vec::new());
        writer.write_all(b"a").unwrap();
        writer.write_all(b"bc").unwrap();
        let (inner, digests) = writer.finish();
        assert_eq!(inner, b"abc");
        assert_eq!(digests.sha256, ABC_SHA256);
    }

    #[test]
    fn compares_declared_algorithms_only() {
        let outcomes = compare_declared(&archive(None, Some(ABC_SHA256), None), &digests_of_abc());
        assert_eq!(
            outcomes,
            vec![ChecksumOutcome {
                algorithm: "sha256",
                declared: ABC_SHA256.to_string(),
                matched: true,
            }]
        );
    }

    #[test]
    fn reports_a_mismatch_per_algorithm() {
        let outcomes = compare_declared(
            &archive(Some(ABC_MD5), None, Some(&"0".repeat(128))),
            &digests_of_abc(),
        );
        let matched: Vec<_> = outcomes.iter().map(|o| (o.algorithm, o.matched)).collect();
        assert_eq!(matched, vec![("md5", true), ("sha512", false)]);
    }

    #[test]
    fn uppercase_declaration_matches() {
        let outcomes = compare_declared(
            &archive(None, Some(&ABC_SHA256.to_uppercase()), None),
            &digests_of_abc(),
        );
        assert!(outcomes[0].matched);
    }

    #[test]
    fn declaring_nothing_compares_nothing() {
        assert!(compare_declared(&archive(None, None, None), &digests_of_abc()).is_empty());
    }
}
