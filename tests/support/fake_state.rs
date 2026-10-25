use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use tiny_http::{Header, Response, Server};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub body: String,
    pub authorization: Option<String>,
}

/// Answers a scripted queue of `(status, body)` pairs in order, and `500` once it runs dry.
pub struct FakeState {
    server: Arc<Server>,
    requests: Arc<Mutex<Vec<Recorded>>>,
    worker: Option<JoinHandle<()>>,
    pub url: String,
}

impl FakeState {
    pub fn answering(script: &[(u16, &str)]) -> Self {
        let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
        let url = format!("http://{}", server.server_addr());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let mut queue: VecDeque<(u16, String)> = script
            .iter()
            .map(|(status, body)| (*status, (*body).to_owned()))
            .collect();
        let worker = {
            let server = Arc::clone(&server);
            let requests = Arc::clone(&requests);
            thread::spawn(move || {
                for mut request in server.incoming_requests() {
                    let mut body = String::new();
                    request.as_reader().read_to_string(&mut body).unwrap();
                    let authorization = request
                        .headers()
                        .iter()
                        .find(|header| header.field.equiv("Authorization"))
                        .map(|header| header.value.to_string());
                    requests.lock().unwrap().push(Recorded {
                        method: request.method().to_string(),
                        path: request.url().to_owned(),
                        body,
                        authorization,
                    });
                    let (status, text) = queue.pop_front().unwrap_or((500, String::new()));
                    let json = Header::from_bytes("Content-Type", "application/json").unwrap();
                    let response = Response::from_string(text)
                        .with_status_code(status)
                        .with_header(json);
                    request.respond(response).unwrap();
                }
            })
        };
        Self {
            server,
            requests,
            worker: Some(worker),
            url,
        }
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for FakeState {
    fn drop(&mut self) {
        self.server.unblock();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
