use std::cell::RefCell;
use std::fmt;
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

pub trait StateWriter {
    fn post_candidate(&self, body: &CandidateBody) -> Result<(), WriteError>;
    fn post_version(&self, body: &VersionBody) -> Result<(), WriteError>;
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

pub struct HttpState {
    base_url: String,
    agent: ureq::Agent,
    credentials: Option<Credentials>,
    token: RefCell<Option<String>>,
}

impl HttpState {
    pub fn new(base_url: &str) -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_global(Some(Duration::from_secs(60)))
            .user_agent(concat!("sdkman-contrib/", env!("CARGO_PKG_VERSION")))
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
        let mut answer = self.post_authorised(path, body)?;
        if answer.status == 401 {
            self.obtain_token().map_err(WriteError::Login)?;
            answer = self.post_authorised(path, body)?;
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

    fn post_authorised(&self, path: &str, body: &impl Serialize) -> Result<Answer, WriteError> {
        let token = self.held_or_new_token().map_err(WriteError::Login)?;
        self.post(path, body, Some(&token))
            .map_err(WriteError::Transport)
    }

    fn post(
        &self,
        path: &str,
        body: &impl Serialize,
        token: Option<&str>,
    ) -> Result<Answer, String> {
        let url = format!("{}{path}", self.base_url);
        let mut request = self.agent.post(&url);
        if let Some(token) = token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        let mut response = request
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
