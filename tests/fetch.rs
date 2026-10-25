use std::fs;
use std::thread;

use sdkman_contrib::digest::HashingWriter;
use sdkman_contrib::fetch::{ArchiveSource, Download, FetchError, HttpArchiveSource};
use tiny_http::{Header, Response, Server};

const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

fn serve_once(response: Response<std::io::Cursor<Vec<u8>>>) -> String {
    let server = Server::http("127.0.0.1:0").unwrap();
    let url = format!("http://{}/jpx-1.0.zip", server.server_addr());
    thread::spawn(move || {
        if let Ok(request) = server.recv() {
            request.respond(response).unwrap();
        }
    });
    url
}

#[test]
fn downloads_bytes_and_digests() {
    let url = serve_once(Response::from_data(b"abc".to_vec()));
    let download = HttpArchiveSource::new().fetch(&url).unwrap();
    assert_eq!(download.digests.sha256, ABC_SHA256);
    assert_eq!(fs::read(download.file.path()).unwrap(), b"abc");
}

#[test]
fn refuses_redirect() {
    let location = Header::from_bytes("Location", "http://127.0.0.1:1/elsewhere").unwrap();
    let url = serve_once(
        Response::from_data(Vec::new())
            .with_status_code(302)
            .with_header(location),
    );
    assert_eq!(
        HttpArchiveSource::new().fetch(&url).err(),
        Some(FetchError::Redirect(302))
    );
}

#[test]
fn rejects_non_200_answer() {
    let url = serve_once(Response::from_data(Vec::new()).with_status_code(404));
    assert_eq!(
        HttpArchiveSource::new().fetch(&url).err(),
        Some(FetchError::Status(404))
    );
}

#[test]
fn from_reader_hashes_what_it_stores() {
    let download = Download::from_reader(&b"abc"[..]).unwrap();
    let mut expected = HashingWriter::new(Vec::new());
    std::io::Write::write_all(&mut expected, b"abc").unwrap();
    assert_eq!(download.digests, expected.finish().1);
    assert_eq!(fs::read(download.file.path()).unwrap(), b"abc");
}

#[test]
fn identifies_itself_as_sdkman_contrib() {
    let server = Server::http("127.0.0.1:0").unwrap();
    let url = format!("http://{}/jpx-1.0.zip", server.server_addr());
    let seen = thread::spawn(move || {
        let request = server.recv().unwrap();
        let agent = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("User-Agent"))
            .map(|h| h.value.to_string());
        request
            .respond(Response::from_data(b"abc".to_vec()))
            .unwrap();
        agent
    });
    HttpArchiveSource::new().fetch(&url).unwrap();
    let expected = format!("sdkman-contrib/{}", env!("CARGO_PKG_VERSION"));
    assert_eq!(seen.join().unwrap(), Some(expected));
}
