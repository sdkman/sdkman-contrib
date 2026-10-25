use std::cell::RefCell;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;

use crate::http::agent_config;
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

pub trait StateWriter {
    fn post_candidate(&self, body: &CandidateBody) -> Result<(), WriteError>;
    fn post_version(&self, body: &VersionBody) -> Result<(), WriteError>;
}

pub trait VendorState: StateWriter {
    fn stable_version(&self, candidate: &str, platform: &str)
    -> Result<Option<String>, WriteError>;
    fn has_version(
        &self,
        candidate: &str,
        version: &str,
        platform: &str,
    ) -> Result<bool, WriteError>;
    fn tag_stable(&self, candidate: &str, version: &str, platform: &str) -> Result<(), WriteError>;
    fn untag_stable(&self, candidate: &str, platform: &str) -> Result<(), WriteError>;
    fn delete_version(
        &self,
        candidate: &str,
        version: &str,
        platform: &str,
    ) -> Result<(), WriteError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    Login(String),
    Transport(String),
    Refused { status: u16, messages: Vec<String> },
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Login(message) | Self::Transport(message) => f.write_str(message),
            Self::Refused { status, messages } => {
                write!(
                    f,
                    "State refused the write with {status}: {}",
                    messages.join("; ")
                )
            }
        }
    }
}

impl std::error::Error for WriteError {}

#[derive(Clone)]
pub struct Credentials {
    pub email: String,
    pub password: String,
}

#[derive(Clone, Copy)]
enum Verb {
    Post,
    Delete,
}

pub struct HttpState {
    base_url: String,
    agent: ureq::Agent,
    credentials: Option<Credentials>,
    token: RefCell<Option<String>>,
}

impl HttpState {
    pub fn new(base_url: &str) -> Self {
        let agent = agent_config()
            .timeout_global(Some(Duration::from_secs(60)))
            .build()
            .into();
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            agent,
            credentials: None,
            token: RefCell::new(None),
        }
    }

    pub fn with_credentials(base_url: &str, credentials: Credentials) -> Self {
        Self {
            credentials: Some(credentials),
            ..Self::new(base_url)
        }
    }

    pub fn login(&self) -> Result<(), String> {
        self.obtain_token().map(|_| ())
    }

    fn obtain_token(&self) -> Result<String, String> {
        let credentials = self
            .credentials
            .as_ref()
            .ok_or_else(|| "no State credentials were given".to_owned())?;
        let answer = self.post(
            "/login",
            &serde_json::json!({
                "email": credentials.email,
                "password": credentials.password,
            }),
            None,
        )?;
        if answer.status != 200 {
            return Err(format!("login failed: State answered {}", answer.status));
        }
        let token = answer
            .body
            .get("token")
            .and_then(Value::as_str)
            .ok_or_else(|| "login failed: State answered no token".to_owned())?
            .to_owned();
        *self.token.borrow_mut() = Some(token.clone());
        Ok(token)
    }

    fn held_or_new_token(&self) -> Result<String, String> {
        let held = self.token.borrow().clone();
        match held {
            Some(token) => Ok(token),
            None => self.obtain_token(),
        }
    }

    fn write(&self, path: &str, body: &impl Serialize, accepted: &[u16]) -> Result<(), WriteError> {
        self.write_with(Verb::Post, path, body, accepted)
    }

    fn write_with(
        &self,
        verb: Verb,
        path: &str,
        body: &impl Serialize,
        accepted: &[u16],
    ) -> Result<(), WriteError> {
        let mut answer = self.send_authorised(verb, path, body)?;
        if answer.status == 401 {
            self.obtain_token().map_err(WriteError::Login)?;
            answer = self.send_authorised(verb, path, body)?;
        }
        if accepted.contains(&answer.status) {
            Ok(())
        } else {
            Err(WriteError::Refused {
                status: answer.status,
                messages: answer.messages(),
            })
        }
    }

    fn send_authorised(
        &self,
        verb: Verb,
        path: &str,
        body: &impl Serialize,
    ) -> Result<Answer, WriteError> {
        let token = self.held_or_new_token().map_err(WriteError::Login)?;
        self.send(verb, path, body, Some(&token))
            .map_err(WriteError::Transport)
    }

    fn post(
        &self,
        path: &str,
        body: &impl Serialize,
        token: Option<&str>,
    ) -> Result<Answer, String> {
        self.send(Verb::Post, path, body, token)
    }

    fn send(
        &self,
        verb: Verb,
        path: &str,
        body: &impl Serialize,
        token: Option<&str>,
    ) -> Result<Answer, String> {
        let url = format!("{}{path}", self.base_url);
        let mut response = match verb {
            Verb::Post => {
                let mut request = self.agent.post(&url);
                if let Some(token) = token {
                    request = request.header("Authorization", format!("Bearer {token}"));
                }
                request.send_json(body)
            }
            Verb::Delete => {
                let mut request = self.agent.delete(&url).force_send_body();
                if let Some(token) = token {
                    request = request.header("Authorization", format!("Bearer {token}"));
                }
                request.send_json(body)
            }
        }
        .map_err(|e| format!("{url} could not be reached: {e}"))?;
        answer_of(&url, &mut response)
    }
}

fn answer_of(url: &str, response: &mut ureq::http::Response<ureq::Body>) -> Result<Answer, String> {
    let status = response.status().as_u16();
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("{url} answer could not be read: {e}"))?;
    let body = serde_json::from_str(&text).unwrap_or(Value::Null);
    Ok(Answer { status, body })
}

static CACHE_BUSTERS: AtomicU64 = AtomicU64::new(0);

fn cache_buster() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let sequence = CACHE_BUSTERS.fetch_add(1, Ordering::Relaxed);
    format!("{nanos}-{sequence}")
}

impl HttpState {
    fn get_past_cache(&self, path_and_query: &str) -> Result<Answer, WriteError> {
        let separator = if path_and_query.contains('?') {
            '&'
        } else {
            '?'
        };
        let url = format!(
            "{}{path_and_query}{separator}_cb={}",
            self.base_url,
            cache_buster()
        );
        let mut response = self
            .agent
            .get(&url)
            .call()
            .map_err(|e| WriteError::Transport(format!("{url} could not be reached: {e}")))?;
        answer_of(&url, &mut response).map_err(WriteError::Transport)
    }
}

fn refused(answer: &Answer) -> WriteError {
    WriteError::Refused {
        status: answer.status,
        messages: answer.messages(),
    }
}

impl StateValidator for HttpState {
    fn validate_version(&self, body: &VersionBody) -> Result<Answer, String> {
        self.post("/validate/versions", body, None)
    }

    fn validate_candidate(&self, body: &CandidateBody) -> Result<Answer, String> {
        self.post("/validate/candidates", body, None)
    }
}

impl StateWriter for HttpState {
    fn post_candidate(&self, body: &CandidateBody) -> Result<(), WriteError> {
        self.write("/admin/candidates", body, &[200, 201])
    }

    fn post_version(&self, body: &VersionBody) -> Result<(), WriteError> {
        self.write("/versions", body, &[204])
    }
}

impl VendorState for HttpState {
    fn stable_version(
        &self,
        candidate: &str,
        platform: &str,
    ) -> Result<Option<String>, WriteError> {
        let answer = self.get_past_cache(&format!(
            "/versions/{candidate}/tags/stable?platform={platform}"
        ))?;
        match answer.status {
            200 => answer
                .body
                .get("version")
                .and_then(Value::as_str)
                .map(|version| Some(version.to_owned()))
                .ok_or_else(|| refused(&answer)),
            404 => Ok(None),
            _ => Err(refused(&answer)),
        }
    }

    fn has_version(
        &self,
        candidate: &str,
        version: &str,
        platform: &str,
    ) -> Result<bool, WriteError> {
        let answer = self.get_past_cache(&format!(
            "/versions/{candidate}/{version}?platform={platform}"
        ))?;
        match answer.status {
            200 => Ok(true),
            404 => Ok(false),
            _ => Err(refused(&answer)),
        }
    }
    fn tag_stable(&self, candidate: &str, version: &str, platform: &str) -> Result<(), WriteError> {
        self.write(
            "/versions/tags",
            &serde_json::json!({
                "candidate": candidate,
                "version": version,
                "platform": platform,
                "tag": "stable",
            }),
            &[204],
        )
    }

    fn untag_stable(&self, candidate: &str, platform: &str) -> Result<(), WriteError> {
        self.write_with(
            Verb::Delete,
            "/versions/tags",
            &serde_json::json!({
                "candidate": candidate,
                "platform": platform,
                "tag": "stable",
            }),
            &[204, 404],
        )
    }

    fn delete_version(
        &self,
        candidate: &str,
        version: &str,
        platform: &str,
    ) -> Result<(), WriteError> {
        self.write_with(
            Verb::Delete,
            "/versions",
            &serde_json::json!({
                "candidate": candidate,
                "version": version,
                "platform": platform,
                "distribution": null,
            }),
            &[204, 404],
        )
    }
}
