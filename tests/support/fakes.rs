use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::fs::File;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use sdkman_contrib::check_digests::{CheckDigests, DigestError, DigestRow, DigestSource};
use sdkman_contrib::fetch::{ArchiveSource, Download, FetchError, Hop, HttpGet, follow_redirects};
use sdkman_contrib::state_api::{Answer, StateValidator, StateWriter, VendorState, WriteError};
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

/// Answers every lookup with its scripted result, and records the commit of each lookup.
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

/// Holds rows and `stable` tags like State, and logs each call as `<call> <platform>`.
/// Fails the one call it was told to, and refuses to delete a row that holds `stable`.
#[derive(Default)]
pub struct FakeVendorState {
    rows: RefCell<BTreeSet<(String, String)>>,
    stable: RefCell<BTreeMap<String, String>>,
    posted: RefCell<Vec<VersionBody>>,
    calls: RefCell<Vec<String>>,
    failing: RefCell<Option<String>>,
}

impl FakeVendorState {
    pub fn with_row(self, version: &str, platform: &str) -> Self {
        self.rows
            .borrow_mut()
            .insert((version.to_owned(), platform.to_owned()));
        self
    }

    pub fn with_stable(self, platform: &str, version: &str) -> Self {
        self.stable
            .borrow_mut()
            .insert(platform.to_owned(), version.to_owned());
        self.with_row(version, platform)
    }

    pub fn failing_on(self, call: &str, platform: &str) -> Self {
        self.failing.replace(Some(format!("{call} {platform}")));
        self
    }

    pub fn rows(&self) -> BTreeSet<(String, String)> {
        self.rows.borrow().clone()
    }

    pub fn stable(&self) -> BTreeMap<String, String> {
        self.stable.borrow().clone()
    }

    pub fn posted(&self) -> Vec<VersionBody> {
        self.posted.borrow().clone()
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }

    fn record(&self, call: &str, platform: &str) -> Result<(), WriteError> {
        let entry = format!("{call} {platform}");
        self.calls.borrow_mut().push(entry.clone());
        if self.failing.borrow().as_deref() == Some(entry.as_str()) {
            return Err(WriteError::Refused {
                status: 500,
                messages: vec![format!("{entry} failed on demand")],
            });
        }
        Ok(())
    }
}

impl StateWriter for FakeVendorState {
    fn post_candidate(&self, _body: &CandidateBody) -> Result<(), WriteError> {
        Ok(())
    }

    fn post_version(&self, body: &VersionBody) -> Result<(), WriteError> {
        self.record("post_version", &body.platform)?;
        self.posted.borrow_mut().push(body.clone());
        self.rows
            .borrow_mut()
            .insert((body.version.clone(), body.platform.clone()));
        Ok(())
    }
}

impl VendorState for FakeVendorState {
    fn stable_version(
        &self,
        _candidate: &str,
        platform: &str,
    ) -> Result<Option<String>, WriteError> {
        self.record("stable_version", platform)?;
        Ok(self.stable.borrow().get(platform).cloned())
    }

    fn has_version(
        &self,
        _candidate: &str,
        version: &str,
        platform: &str,
    ) -> Result<bool, WriteError> {
        self.record("has_version", platform)?;
        Ok(self
            .rows
            .borrow()
            .contains(&(version.to_owned(), platform.to_owned())))
    }

    fn tag_stable(
        &self,
        _candidate: &str,
        version: &str,
        platform: &str,
    ) -> Result<(), WriteError> {
        self.record("tag_stable", platform)?;
        self.stable
            .borrow_mut()
            .insert(platform.to_owned(), version.to_owned());
        Ok(())
    }

    fn untag_stable(&self, _candidate: &str, platform: &str) -> Result<(), WriteError> {
        self.record("untag_stable", platform)?;
        self.stable.borrow_mut().remove(platform);
        Ok(())
    }

    fn delete_version(
        &self,
        _candidate: &str,
        version: &str,
        platform: &str,
    ) -> Result<(), WriteError> {
        self.record("delete_version", platform)?;
        if self.stable.borrow().get(platform).map(String::as_str) == Some(version) {
            return Err(WriteError::Refused {
                status: 409,
                messages: vec![format!("{version} of {platform} holds a tag")],
            });
        }
        self.rows
            .borrow_mut()
            .remove(&(version.to_owned(), platform.to_owned()));
        Ok(())
    }
}
