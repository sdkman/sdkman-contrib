use std::collections::HashMap;
use std::io::Cursor;

use tiny_http::{Header, Response, StatusCode};

use super::loopback::{Loopback, Recorded};

#[derive(Clone)]
pub struct Reply {
    pub status: u16,
    pub body: Vec<u8>,
    pub location: Option<String>,
}

impl Reply {
    pub fn json(body: &str) -> Self {
        Self {
            status: 200,
            body: body.as_bytes().to_vec(),
            location: None,
        }
    }

    pub fn bytes(body: Vec<u8>) -> Self {
        Self {
            status: 200,
            body,
            location: None,
        }
    }

    pub fn redirect(location: &str) -> Self {
        Self {
            status: 302,
            body: Vec::new(),
            location: Some(location.to_owned()),
        }
    }

    pub fn status(status: u16) -> Self {
        Self {
            status,
            body: Vec::new(),
            location: None,
        }
    }
}

/// Answers each request by its path and query, and `404` for a path it was not given.
/// A redirect names a path on this server; the fake turns it into an absolute URL.
pub struct FakeGitHub {
    server: Loopback,
    pub url: String,
}

impl FakeGitHub {
    pub fn routed(routes: Vec<(&str, Reply)>) -> Self {
        let routes: HashMap<String, Reply> = routes
            .into_iter()
            .map(|(path, reply)| (path.to_owned(), reply))
            .collect();
        let server = Loopback::serving(move |origin, request| {
            let Reply {
                status,
                body,
                location,
            } = routes
                .get(&request.path)
                .cloned()
                .unwrap_or_else(|| Reply::status(404));
            let length = body.len();
            let mut response = Response::new(
                StatusCode(status),
                Vec::new(),
                Cursor::new(body),
                Some(length),
                None,
            );
            if let Some(location) = location {
                let absolute = format!("{origin}{location}");
                response.add_header(Header::from_bytes("Location", absolute).unwrap());
            }
            response
        });
        Self {
            url: server.url.clone(),
            server,
        }
    }

    pub fn seen(&self) -> Vec<Recorded> {
        self.server.requests()
    }

    pub fn paths(&self) -> Vec<String> {
        self.server.paths()
    }
}
