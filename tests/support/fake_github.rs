use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use tiny_http::{Header, Response, Server};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub path: String,
    pub authorization: Option<String>,
}

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
    server: Arc<Server>,
    seen: Arc<Mutex<Vec<Seen>>>,
    worker: Option<JoinHandle<()>>,
    pub url: String,
}

impl FakeGitHub {
    pub fn routed(routes: Vec<(&str, Reply)>) -> Self {
        let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
        let url = format!("http://{}", server.server_addr());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let origin = url.clone();
        let routes: HashMap<String, Reply> = routes
            .into_iter()
            .map(|(path, reply)| (path.to_owned(), reply))
            .collect();
        let worker = {
            let server = Arc::clone(&server);
            let seen = Arc::clone(&seen);
            thread::spawn(move || {
                for request in server.incoming_requests() {
                    let authorization = request
                        .headers()
                        .iter()
                        .find(|header| header.field.equiv("Authorization"))
                        .map(|header| header.value.to_string());
                    seen.lock().unwrap().push(Seen {
                        path: request.url().to_owned(),
                        authorization,
                    });
                    let reply = routes
                        .get(request.url())
                        .cloned()
                        .unwrap_or_else(|| Reply::status(404));
                    let mut response = Response::new(
                        tiny_http::StatusCode(reply.status),
                        Vec::new(),
                        Cursor::new(reply.body.clone()),
                        Some(reply.body.len()),
                        None,
                    );
                    if let Some(location) = &reply.location {
                        let absolute = format!("{origin}{location}");
                        response.add_header(Header::from_bytes("Location", absolute).unwrap());
                    }
                    request.respond(response).unwrap();
                }
            })
        };
        Self {
            server,
            seen,
            worker: Some(worker),
            url,
        }
    }

    pub fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }

    pub fn paths(&self) -> Vec<String> {
        self.seen().into_iter().map(|seen| seen.path).collect()
    }
}

impl Drop for FakeGitHub {
    fn drop(&mut self) {
        self.server.unblock();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
