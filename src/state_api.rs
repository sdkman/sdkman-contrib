use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use crate::state_body::{CandidateBody, VersionBody};

pub const DEFAULT_STATE_URL: &str = "https://state.sdkman.io";

pub fn resolve_state_url(configured: Option<String>) -> String {
    configured
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| DEFAULT_STATE_URL.to_owned())
}

#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub status: u16,
    pub body: Value,
}

impl Answer {
    pub fn passed(&self) -> bool {
        self.status == 204
    }

    pub fn messages(&self) -> Vec<String> {
        if let Some(failures) = self.body.get("failures").and_then(Value::as_array) {
            let listed: Vec<String> = failures.iter().filter_map(failure_message).collect();
            if !listed.is_empty() {
                return listed;
            }
        }
        match self.body.get("message").and_then(Value::as_str) {
            Some(message) => vec![message.to_owned()],
            None => vec![format!("State answered {}", self.status)],
        }
    }
}

fn failure_message(failure: &Value) -> Option<String> {
    let message = failure.get("message")?.as_str()?;
    match failure.get("field").and_then(Value::as_str) {
        Some(field) => Some(format!("{field}: {message}")),
        None => Some(message.to_owned()),
    }
}

pub trait StateValidator {
    fn validate_version(&self, body: &VersionBody) -> Result<Answer, String>;
    fn validate_candidate(&self, body: &CandidateBody) -> Result<Answer, String>;
}

pub struct HttpState {
    base_url: String,
    agent: ureq::Agent,
}

impl HttpState {
    pub fn new(base_url: &str) -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(30)))
            .user_agent(concat!("sdkman-contrib/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            agent,
        }
    }

    fn post(&self, path: &str, body: &impl Serialize) -> Result<Answer, String> {
        let url = format!("{}{path}", self.base_url);
        let mut response = self
            .agent
            .post(&url)
            .send_json(body)
            .map_err(|e| format!("{url} could not be reached: {e}"))?;
        let status = response.status().as_u16();
        let text = response
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("{url} answer could not be read: {e}"))?;
        let body = serde_json::from_str(&text).unwrap_or(Value::Null);
        Ok(Answer { status, body })
    }
}

impl StateValidator for HttpState {
    fn validate_version(&self, body: &VersionBody) -> Result<Answer, String> {
        self.post("/validate/versions", body)
    }

    fn validate_candidate(&self, body: &CandidateBody) -> Result<Answer, String> {
        self.post("/validate/candidates", body)
    }
}
