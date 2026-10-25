use std::cell::Cell;
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};

use sdkman_contrib::fetch::{ArchiveSource, Download, FetchError};

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
