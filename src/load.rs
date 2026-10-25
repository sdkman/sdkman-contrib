use std::fs;
use std::path::Path;

use crate::listing::CandidateListing;
use crate::request::{Request, parse_request};
use crate::request_path::RequestPath;

pub fn load_request(
    repo_root: &Path,
    repo_path: &str,
) -> Result<(RequestPath, Request), Vec<String>> {
    let path = RequestPath::parse(repo_path).map_err(|error| vec![error.to_string()])?;
    let text = fs::read_to_string(repo_root.join(repo_path))
        .map_err(|error| vec![format!("cannot read {repo_path}: {error}")])?;
    let request = parse_request(&text).map_err(|error| vec![error.to_string()])?;
    let mismatches = request.mismatches(&path);
    if !mismatches.is_empty() {
        return Err(mismatches);
    }
    let listing = CandidateListing::load(repo_root)
        .map_err(|error| vec![format!("cannot read the candidate listing: {error}")])?;
    if let Some(refusal) = listing.refusal(request.candidate()) {
        return Err(vec![refusal]);
    }
    if let Request::Release(release) = &request {
        let errors = release.structure_errors();
        if !errors.is_empty() {
            return Err(errors);
        }
    }
    Ok((path, request))
}
