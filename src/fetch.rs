use std::fmt;
use std::io::{self, Read};
use std::time::Duration;

use tempfile::NamedTempFile;

use crate::digest::{Digests, HashingWriter};

pub struct Download {
    pub file: NamedTempFile,
    pub digests: Digests,
}

impl Download {
    pub fn from_reader(mut reader: impl Read) -> io::Result<Self> {
        let mut writer = HashingWriter::new(NamedTempFile::new()?);
        io::copy(&mut reader, &mut writer)?;
        let (file, digests) = writer.finish();
        Ok(Self { file, digests })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum FetchError {
    Redirect(u16),
    Status(u16),
    Transport(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Redirect(status) => write!(f, "answered a redirect ({status})"),
            Self::Status(status) => write!(f, "answered {status}, expected 200"),
            Self::Transport(message) => write!(f, "could not be downloaded: {message}"),
        }
    }
}

pub trait ArchiveSource {
    fn fetch(&self, url: &str) -> Result<Download, FetchError>;
}

pub struct HttpArchiveSource {
    agent: ureq::Agent,
}

impl HttpArchiveSource {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(30)))
            .user_agent(concat!("sdkman-contrib/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Self { agent }
    }
}

impl Default for HttpArchiveSource {
    fn default() -> Self {
        Self::new()
    }
}

impl ArchiveSource for HttpArchiveSource {
    fn fetch(&self, url: &str) -> Result<Download, FetchError> {
        let response = self
            .agent
            .get(url)
            .call()
            .map_err(|e| FetchError::Transport(e.to_string()))?;
        match response.status().as_u16() {
            200 => Download::from_reader(response.into_body().into_reader())
                .map_err(|e| FetchError::Transport(e.to_string())),
            status @ 300..=399 => Err(FetchError::Redirect(status)),
            status => Err(FetchError::Status(status)),
        }
    }
}
