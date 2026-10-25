use std::io::{Cursor, Read};
use std::time::Duration;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::check_digests::{CheckDigests, DigestError, DigestSource};
use crate::http::agent_config;

const TRUSTED_EVENT: &str = "pull_request_target";
const TRUSTED_WORKFLOW: &str = ".github/workflows/request-check.yml";
const DIGESTS_FILE: &str = "digests.json";
const BASE_BRANCH: &str = "main";

#[derive(Deserialize)]
struct PullRequest {
    number: u64,
    merged_at: Option<String>,
    base: Ref,
    head: Head,
}

#[derive(Deserialize)]
struct Ref {
    #[serde(rename = "ref")]
    name: String,
}

#[derive(Deserialize)]
struct Head {
    sha: String,
}

#[derive(Deserialize)]
struct ArtifactList {
    artifacts: Vec<Artifact>,
}

#[derive(Deserialize)]
struct Artifact {
    id: u64,
    expired: bool,
    created_at: String,
    workflow_run: ArtifactRun,
}

#[derive(Deserialize)]
struct ArtifactRun {
    id: u64,
}

#[derive(Deserialize)]
struct WorkflowRun {
    event: String,
    conclusion: Option<String>,
    path: String,
}

impl WorkflowRun {
    fn is_trusted_check(&self) -> bool {
        let workflow = self.path.split('@').next().unwrap_or_default();
        self.event == TRUSTED_EVENT
            && self.conclusion.as_deref() == Some("success")
            && workflow == TRUSTED_WORKFLOW
    }
}

pub struct GitHubDigests {
    api_url: String,
    repository: String,
    token: String,
    agent: ureq::Agent,
}

impl GitHubDigests {
    pub fn new(api_url: &str, repository: &str, token: &str) -> Self {
        let agent = agent_config()
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(60)))
            .build()
            .into();
        Self {
            api_url: api_url.trim_end_matches('/').to_owned(),
            repository: repository.to_owned(),
            token: token.to_owned(),
            agent,
        }
    }

    fn repo_url(&self, path: &str) -> String {
        format!("{}/repos/{}/{path}", self.api_url, self.repository)
    }

    fn get_authorised(&self, url: &str) -> Result<ureq::http::Response<ureq::Body>, DigestError> {
        self.agent
            .get(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .call()
            .map_err(|e| DigestError::Failed(format!("{url} could not be reached: {e}")))
    }

    fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T, DigestError> {
        let mut response = self.get_authorised(url)?;
        expect_status(url, response.status().as_u16(), 200)?;
        response
            .body_mut()
            .read_json()
            .map_err(|e| DigestError::Failed(format!("{url} answer could not be read: {e}")))
    }

    fn merged_pull_request(&self, commit: &str) -> Result<Option<PullRequest>, DigestError> {
        let pulls: Vec<PullRequest> =
            self.get_json(&self.repo_url(&format!("commits/{commit}/pulls")))?;
        Ok(pulls
            .into_iter()
            .find(|pull| pull.merged_at.is_some() && pull.base.name == BASE_BRANCH))
    }

    fn unexpired_artifacts(&self, name: &str) -> Result<Vec<Artifact>, DigestError> {
        let list: ArtifactList =
            self.get_json(&self.repo_url(&format!("actions/artifacts?name={name}")))?;
        let mut artifacts: Vec<Artifact> = list
            .artifacts
            .into_iter()
            .filter(|artifact| !artifact.expired)
            .collect();
        artifacts.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(artifacts)
    }

    fn ran_trusted_check(&self, artifact: &Artifact) -> Result<bool, DigestError> {
        let run: WorkflowRun =
            self.get_json(&self.repo_url(&format!("actions/runs/{}", artifact.workflow_run.id)))?;
        Ok(run.is_trusted_check())
    }

    fn download_digests(&self, artifact: &Artifact) -> Result<CheckDigests, DigestError> {
        let zip_url = self.repo_url(&format!("actions/artifacts/{}/zip", artifact.id));
        let redirect = self.get_authorised(&zip_url)?;
        expect_status(&zip_url, redirect.status().as_u16(), 302)?;
        let blob_url = redirect
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| DigestError::Failed(format!("{zip_url} redirected without a Location")))?
            .to_owned();
        let mut blob =
            self.agent.get(&blob_url).call().map_err(|e| {
                DigestError::Failed(format!("{blob_url} could not be reached: {e}"))
            })?;
        expect_status(&blob_url, blob.status().as_u16(), 200)?;
        let bytes = blob.body_mut().read_to_vec().map_err(|e| {
            DigestError::Failed(format!("{blob_url} answer could not be read: {e}"))
        })?;
        parse_digests(&bytes)
    }
}

fn expect_status(url: &str, status: u16, expected: u16) -> Result<(), DigestError> {
    if status == expected {
        Ok(())
    } else {
        Err(DigestError::Failed(format!(
            "{url} answered {status}, expected {expected}"
        )))
    }
}

fn parse_digests(zip_bytes: &[u8]) -> Result<CheckDigests, DigestError> {
    let failed = |reason: String| DigestError::Failed(format!("{DIGESTS_FILE} artifact {reason}"));
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))
        .map_err(|e| failed(format!("is not a zip: {e}")))?;
    let mut file = archive
        .by_name(DIGESTS_FILE)
        .map_err(|e| failed(format!("holds no {DIGESTS_FILE}: {e}")))?;
    let mut text = String::new();
    file.read_to_string(&mut text)
        .map_err(|e| failed(format!("could not be read: {e}")))?;
    serde_json::from_str(&text).map_err(|e| failed(format!("is not valid: {e}")))
}

impl DigestSource for GitHubDigests {
    fn reviewed_digests(&self, commit: &str) -> Result<CheckDigests, DigestError> {
        let Some(pull) = self.merged_pull_request(commit)? else {
            return Err(DigestError::Missing);
        };
        let name = format!("digests-{}-{}", pull.number, pull.head.sha);
        for artifact in self.unexpired_artifacts(&name)? {
            if self.ran_trusted_check(&artifact)? {
                return self.download_digests(&artifact);
            }
        }
        Err(DigestError::Missing)
    }
}
