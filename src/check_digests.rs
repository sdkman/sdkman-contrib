use std::fmt;

use serde::{Deserialize, Serialize};

use crate::verify::RowCheck;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckDigests {
    pub request: String,
    pub rows: Vec<DigestRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DigestRow {
    pub version: String,
    pub platform: String,
    pub sha256: String,
}

impl DigestRow {
    pub fn from_check(row: &RowCheck) -> Option<Self> {
        row.digests.as_ref().map(|digests| Self {
            version: row.version.clone(),
            platform: row.platform.clone(),
            sha256: digests.sha256.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DigestError {
    Missing,
    Failed(String),
}

impl fmt::Display for DigestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => f.write_str("no reviewed digests were found for the merged commit"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for DigestError {}

pub trait DigestSource {
    fn reviewed_digests(&self, commit: &str) -> Result<CheckDigests, DigestError>;
}
