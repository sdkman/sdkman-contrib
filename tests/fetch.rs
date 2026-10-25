use std::fs;
use std::thread;
use std::time::Duration;

use sdkman_contrib::digest::HashingWriter;
use sdkman_contrib::fetch::{ArchiveSource, Download, FetchError, HttpArchiveSource};
use tiny_http::{Header, Response, Server};

mod support;
use support::fakes::FakeHops;

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
fn refuses_redirect_to_http() {
    let location = Header::from_bytes("Location", "http://127.0.0.1:1/elsewhere").unwrap();
    let url = serve_once(
        Response::from_data(Vec::new())
            .with_status_code(302)
            .with_header(location),
    );
    assert_eq!(
        HttpArchiveSource::new().fetch(&url).err(),
        Some(FetchError::InsecureRedirect)
    );
}

fn hop_url(n: usize) -> String {
    format!("https://example.test/hop{n}")
}

fn chain_of(redirects: usize) -> FakeHops {
    let hops = (0..redirects).fold(FakeHops::default(), |hops, n| {
        hops.redirecting(&hop_url(n), &hop_url(n + 1))
    });
    hops.serving(&hop_url(redirects), b"abc")
}

#[test]
fn five_https_redirects_pass() {
    let download = chain_of(5).fetch(&hop_url(0)).unwrap();
    assert_eq!(download.digests.sha256, ABC_SHA256);
}

#[test]
fn sixth_redirect_fails() {
    assert_eq!(
        chain_of(6).fetch(&hop_url(0)).err(),
        Some(FetchError::TooManyRedirects)
    );
}

#[test]
fn http_hop_fails() {
    let hops = FakeHops::default()
        .redirecting(&hop_url(0), "http://example.test/archive.zip")
        .serving("http://example.test/archive.zip", b"abc");
    assert_eq!(
        hops.fetch(&hop_url(0)).err(),
        Some(FetchError::InsecureRedirect)
    );
    assert_eq!(hops.requested(), vec![hop_url(0)]);
}

#[test]
fn redirect_without_location_fails() {
    let hops = FakeHops::default().redirecting_without_location(&hop_url(0));
    assert_eq!(hops.fetch(&hop_url(0)).err(), Some(FetchError::BadRedirect));
}

#[test]
fn unparsable_location_fails() {
    let hops = FakeHops::default().redirecting(&hop_url(0), "https://");
    assert_eq!(hops.fetch(&hop_url(0)).err(), Some(FetchError::BadRedirect));
}

#[test]
fn relative_location_resolves_against_the_current_url() {
    let hops = FakeHops::default()
        .redirecting("https://example.test/a/start", "../files/jpx.zip")
        .serving("https://example.test/files/jpx.zip", b"abc");
    assert!(hops.fetch("https://example.test/a/start").is_ok());
}

#[test]
fn redirect_chain_ending_in_404_fails() {
    let hops = FakeHops::default().redirecting(&hop_url(0), &hop_url(1));
    assert_eq!(hops.fetch(&hop_url(0)).err(), Some(FetchError::Status(404)));
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

#[test]
fn stalled_server_is_a_transport_error() {
    let server = Server::http("127.0.0.1:0").unwrap();
    let url = format!("http://{}/jpx-1.0.zip", server.server_addr());
    let stalled = thread::spawn(move || {
        let held = server.recv().unwrap();
        thread::sleep(Duration::from_secs(3));
        drop(held);
    });
    let error = HttpArchiveSource::with_response_timeout(Duration::from_millis(300))
        .fetch(&url)
        .err();
    assert!(matches!(error, Some(FetchError::Transport(_))), "{error:?}");
    stalled.join().unwrap();
}

#[test]
fn closed_port_is_a_transport_error() {
    let server = Server::http("127.0.0.1:0").unwrap();
    let url = format!("http://{}/jpx-1.0.zip", server.server_addr());
    drop(server);
    let error = HttpArchiveSource::new().fetch(&url).err();
    assert!(matches!(error, Some(FetchError::Transport(_))), "{error:?}");
}
