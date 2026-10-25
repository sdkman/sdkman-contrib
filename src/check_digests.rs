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
