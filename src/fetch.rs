use std::fmt;
use std::io::{self, Read};
use std::time::Duration;

use tempfile::NamedTempFile;
use url::Url;

use crate::digest::{Digests, HashingWriter};
use crate::http::agent_config;

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
    TooManyRedirects,
    InsecureRedirect,
    BadRedirect,
    Status(u16),
    Transport(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyRedirects => write!(f, "redirected more than {MAX_REDIRECTS} times"),
            Self::InsecureRedirect => write!(f, "redirected to a URL that is not https"),
            Self::BadRedirect => write!(f, "redirected without a usable Location"),
            Self::Status(status) => write!(f, "answered {status}, expected 200"),
            Self::Transport(message) => write!(f, "could not be downloaded: {message}"),
        }
    }
}

pub const MAX_REDIRECTS: usize = 5;

pub trait ArchiveSource {
    fn fetch(&self, url: &str) -> Result<Download, FetchError>;
}

pub struct Hop {
    pub status: u16,
    pub location: Option<String>,
    pub body: Box<dyn Read>,
}

pub trait HttpGet {
    fn get(&self, url: &str) -> Result<Hop, FetchError>;
}

pub fn follow_redirects(source: &dyn HttpGet, url: &str) -> Result<Download, FetchError> {
    let mut target = url.to_owned();
    let mut redirects = 0;
    loop {
        let hop = source.get(&target)?;
        match hop.status {
            200 => {
                return Download::from_reader(hop.body)
                    .map_err(|e| FetchError::Transport(e.to_string()));
            }
            300..=399 => {
                if redirects == MAX_REDIRECTS {
                    return Err(FetchError::TooManyRedirects);
                }
                redirects += 1;
                let location = hop.location.ok_or(FetchError::BadRedirect)?;
                let current = Url::parse(&target).map_err(|_| FetchError::BadRedirect)?;
                let next = current
                    .join(&location)
                    .map_err(|_| FetchError::BadRedirect)?;
                if next.scheme() != "https" {
                    return Err(FetchError::InsecureRedirect);
                }
                target = next.into();
            }
            status => return Err(FetchError::Status(status)),
        }
    }
}

pub struct HttpArchiveSource {
    agent: ureq::Agent,
}

impl HttpArchiveSource {
    pub fn new() -> Self {
        Self::with_response_timeout(Duration::from_secs(30))
    }

    pub fn with_response_timeout(response_timeout: Duration) -> Self {
        let agent = agent_config()
            .max_redirects(0)
            .timeout_recv_response(Some(response_timeout))
            .timeout_per_read(Some(Duration::from_secs(60)))
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

impl HttpGet for HttpArchiveSource {
    fn get(&self, url: &str) -> Result<Hop, FetchError> {
        let response = self
            .agent
            .get(url)
            .call()
            .map_err(|e| FetchError::Transport(e.to_string()))?;
        let location = response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        Ok(Hop {
            status: response.status().as_u16(),
            location,
            body: Box::new(response.into_body().into_reader()),
        })
    }
}

impl ArchiveSource for HttpArchiveSource {
    fn fetch(&self, url: &str) -> Result<Download, FetchError> {
        follow_redirects(self, url)
    }
}
