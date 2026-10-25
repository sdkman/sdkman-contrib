use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use tiny_http::{Response, Server};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub body: String,
    pub authorization: Option<String>,
}

/// Records every request on `127.0.0.1` and answers it with the handler, which also gets the
/// server's own URL. Dropping it stops the server.
pub struct Loopback {
    server: Arc<Server>,
    requests: Arc<Mutex<Vec<Recorded>>>,
    worker: Option<JoinHandle<()>>,
    pub url: String,
}

impl Loopback {
    pub fn serving<H>(mut handler: H) -> Self
    where
        H: FnMut(&str, &Recorded) -> Response<Cursor<Vec<u8>>> + Send + 'static,
    {
        let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
        let url = format!("http://{}", server.server_addr());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let worker = {
            let server = Arc::clone(&server);
            let requests = Arc::clone(&requests);
            let origin = url.clone();
            thread::spawn(move || {
                for mut request in server.incoming_requests() {
                    let mut body = String::new();
                    request.as_reader().read_to_string(&mut body).unwrap();
                    let authorization = request
                        .headers()
                        .iter()
                        .find(|header| header.field.equiv("Authorization"))
                        .map(|header| header.value.to_string());
                    let recorded = Recorded {
                        method: request.method().to_string(),
                        path: request.url().to_owned(),
                        body,
                        authorization,
                    };
                    let response = handler(&origin, &recorded);
                    requests.lock().unwrap().push(recorded);
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

    pub fn paths(&self) -> Vec<String> {
        self.requests().into_iter().map(|r| r.path).collect()
    }
}

impl Drop for Loopback {
    fn drop(&mut self) {
        self.server.unblock();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
