use std::collections::VecDeque;

use tiny_http::{Header, Response};

use super::loopback::{Loopback, Recorded};

/// Answers a scripted queue of `(status, body)` pairs in order, and `500` once it runs dry.
pub struct FakeState {
    server: Loopback,
    pub url: String,
}

impl FakeState {
    pub fn answering(script: &[(u16, &str)]) -> Self {
        let mut queue: VecDeque<(u16, String)> = script
            .iter()
            .map(|(status, body)| (*status, (*body).to_owned()))
            .collect();
        let server = Loopback::serving(move |_, _| {
            let (status, text) = queue.pop_front().unwrap_or((500, String::new()));
            let json = Header::from_bytes("Content-Type", "application/json").unwrap();
            Response::from_string(text)
                .with_status_code(status)
                .with_header(json)
        });
        Self {
            url: server.url.clone(),
            server,
        }
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.server.requests()
    }

    pub fn paths(&self) -> Vec<String> {
        self.server.paths()
    }
}
