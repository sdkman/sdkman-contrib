use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs::File;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use sdkman_contrib::check_digests::{CheckDigests, DigestError, DigestRow, DigestSource};
use sdkman_contrib::fetch::{ArchiveSource, Download, FetchError, Hop, HttpGet, follow_redirects};
use sdkman_contrib::state_api::{Answer, StateValidator, StateWriter, WriteError};
use sdkman_contrib::state_body::{CandidateBody, VersionBody};

/// Answers a URL with the bytes of a local file, and `404` for any URL it was not given.
#[derive(Default)]
pub struct FakeArchives {
    files: HashMap<String, PathBuf>,
    fetches: Cell<usize>,
}

impl FakeArchives {
    pub fn serving(mut self, url: &str, file: &Path) -> Self {
        self.files.insert(url.to_owned(), file.to_path_buf());
        self
    }

    pub fn fetch_count(&self) -> usize {
        self.fetches.get()
    }
}

impl ArchiveSource for FakeArchives {
    fn fetch(&self, url: &str) -> Result<Download, FetchError> {
        self.fetches.set(self.fetches.get() + 1);
        let path = self.files.get(url).ok_or(FetchError::Status(404))?;
        let file = File::open(path).map_err(|error| FetchError::Transport(error.to_string()))?;
        Download::from_reader(file).map_err(|error| FetchError::Transport(error.to_string()))
    }
}

enum Scripted {
    Redirect(Option<String>),
    Bytes(Vec<u8>),
}

/// Answers each URL with its scripted hop, and `404` for any URL it was not given.
#[derive(Default)]
pub struct FakeHops {
    hops: HashMap<String, Scripted>,
    requested: RefCell<Vec<String>>,
}

impl FakeHops {
    pub fn redirecting(mut self, from: &str, location: &str) -> Self {
        self.hops.insert(
            from.to_owned(),
            Scripted::Redirect(Some(location.to_owned())),
        );
        self
    }

    pub fn redirecting_without_location(mut self, from: &str) -> Self {
        self.hops.insert(from.to_owned(), Scripted::Redirect(None));
        self
    }

    pub fn serving(mut self, url: &str, bytes: &[u8]) -> Self {
        self.hops
            .insert(url.to_owned(), Scripted::Bytes(bytes.to_vec()));
        self
    }

    pub fn requested(&self) -> Vec<String> {
        self.requested.borrow().clone()
    }
}

impl HttpGet for FakeHops {
    fn get(&self, url: &str) -> Result<Hop, FetchError> {
        self.requested.borrow_mut().push(url.to_owned());
        let (status, location, bytes) = match self.hops.get(url) {
            Some(Scripted::Redirect(location)) => (302, location.clone(), Vec::new()),
            Some(Scripted::Bytes(bytes)) => (200, None, bytes.clone()),
            None => (404, None, Vec::new()),
        };
        Ok(Hop {
            status,
            location,
            body: Box::new(Cursor::new(bytes)),
        })
    }
}

impl ArchiveSource for FakeHops {
    fn fetch(&self, url: &str) -> Result<Download, FetchError> {
        follow_redirects(self, url)
    }
}

/// Records every body it is asked to validate. Answers its scripted results in order, and `204` once the script runs dry.
#[derive(Default)]
pub struct FakeValidator {
    script: RefCell<VecDeque<Result<Answer, String>>>,
    versions: RefCell<Vec<VersionBody>>,
    candidates: RefCell<Vec<CandidateBody>>,
}

impl FakeValidator {
    pub fn answering(statuses: &[u16]) -> Self {
        statuses.iter().fold(Self::default(), |fake, status| {
            fake.then(Ok(Answer {
                status: *status,
                body: serde_json::Value::Null,
            }))
        })
    }

    pub fn then(self, result: Result<Answer, String>) -> Self {
        self.script.borrow_mut().push_back(result);
        self
    }

    pub fn versions(&self) -> Vec<VersionBody> {
        self.versions.borrow().clone()
    }

    pub fn candidates(&self) -> Vec<CandidateBody> {
        self.candidates.borrow().clone()
    }

    fn next(&self) -> Result<Answer, String> {
        self.script.borrow_mut().pop_front().unwrap_or(Ok(Answer {
            status: 204,
            body: serde_json::Value::Null,
        }))
    }
}

impl StateValidator for FakeValidator {
    fn validate_version(&self, body: &VersionBody) -> Result<Answer, String> {
        self.versions.borrow_mut().push(body.clone());
        self.next()
    }

    fn validate_candidate(&self, body: &CandidateBody) -> Result<Answer, String> {
        self.candidates.borrow_mut().push(body.clone());
        self.next()
    }
}

/// Keeps the last body posted for each version and platform, as State's upsert does. Refuses the row it was told to fail on.
#[derive(Default)]
pub struct FakeWriter {
    candidates: RefCell<Vec<CandidateBody>>,
    rows: RefCell<BTreeMap<(String, String), VersionBody>>,
    attempts: Cell<usize>,
    failing_row: RefCell<Option<(String, String)>>,
}

impl FakeWriter {
    pub fn failing_on(version: &str, platform: &str) -> Self {
        Self {
            failing_row: RefCell::new(Some((version.to_owned(), platform.to_owned()))),
            ..Self::default()
        }
    }

    pub fn stop_failing(&self) {
        self.failing_row.replace(None);
    }

    pub fn rows(&self) -> BTreeMap<(String, String), VersionBody> {
        self.rows.borrow().clone()
    }

    pub fn candidates(&self) -> Vec<CandidateBody> {
        self.candidates.borrow().clone()
    }

    pub fn write_attempts(&self) -> usize {
        self.attempts.get()
    }
}

impl StateWriter for FakeWriter {
    fn post_candidate(&self, body: &CandidateBody) -> Result<(), WriteError> {
        self.attempts.set(self.attempts.get() + 1);
        self.candidates.borrow_mut().push(body.clone());
        Ok(())
    }

    fn post_version(&self, body: &VersionBody) -> Result<(), WriteError> {
        self.attempts.set(self.attempts.get() + 1);
        let key = (body.version.clone(), body.platform.clone());
        if self.failing_row.borrow().as_ref() == Some(&key) {
            return Err(WriteError::Refused {
                status: 400,
                messages: vec!["refused on demand".to_owned()],
            });
        }
        self.rows.borrow_mut().insert(key, body.clone());
        Ok(())
    }
}

/// Answers every lookup with its scripted result, and counts the lookups.
pub struct FakeDigests {
    answer: Result<CheckDigests, DigestError>,
    lookups: RefCell<Vec<String>>,
}

impl FakeDigests {
    pub fn with_rows(rows: Vec<DigestRow>) -> Self {
        Self::answering(Ok(CheckDigests {
            request: "requests/jpx/reviewed.yaml".to_owned(),
            rows,
        }))
    }

    pub fn missing() -> Self {
        Self::answering(Err(DigestError::Missing))
    }

    pub fn failing(message: &str) -> Self {
        Self::answering(Err(DigestError::Failed(message.to_owned())))
    }

    fn answering(answer: Result<CheckDigests, DigestError>) -> Self {
        Self {
            answer,
            lookups: RefCell::new(Vec::new()),
        }
    }

    pub fn lookups(&self) -> Vec<String> {
        self.lookups.borrow().clone()
    }
}

impl DigestSource for FakeDigests {
    fn reviewed_digests(&self, commit: &str) -> Result<CheckDigests, DigestError> {
        self.lookups.borrow_mut().push(commit.to_owned());
        self.answer.clone()
    }
}
