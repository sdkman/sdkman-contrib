mod support;

use sdkman_contrib::state_api::{
    Answer, DEFAULT_STATE_URL, HttpState, StateValidator, resolve_state_url,
};
use sdkman_contrib::state_body::{CandidateBody, VersionBody};
use serde_json::{Value, json};
use support::fake_state::FakeState;

fn version_body() -> VersionBody {
    VersionBody {
        candidate: "jpx".to_owned(),
        version: "1.2.0".to_owned(),
        platform: "LINUX_X64".to_owned(),
        url: "https://example.com/jpx-1.2.0.zip".to_owned(),
        md5sum: None,
        sha256sum: "ab".repeat(32),
        sha512sum: None,
        tags: Some(vec!["stable".to_owned()]),
    }
}

fn candidate_body() -> CandidateBody {
    CandidateBody {
        candidate: "jpx".to_owned(),
        name: "JPX".to_owned(),
        description: "A tool".to_owned(),
        website_url: "https://example.com".to_owned(),
    }
}

#[test]
fn status_204_passes() {
    let state = FakeState::answering(&[(204, "")]);
    let answer = HttpState::new(&state.url)
        .validate_version(&version_body())
        .unwrap();
    assert!(answer.passed());
}

#[test]
fn status_400_lists_messages() {
    let body = r#"{"error":"Validation Error","failures":[{"field":"version","message":"bad"},{"field":"url","message":"worse"}]}"#;
    let state = FakeState::answering(&[(400, body)]);
    let answer = HttpState::new(&state.url)
        .validate_version(&version_body())
        .unwrap();
    assert!(!answer.passed());
    assert_eq!(answer.messages(), vec!["version: bad", "url: worse"]);
}

#[test]
fn status_404_fails_with_status_message() {
    let state = FakeState::answering(&[(404, "")]);
    let answer = HttpState::new(&state.url)
        .validate_candidate(&candidate_body())
        .unwrap();
    assert!(!answer.passed());
    assert_eq!(answer.messages(), vec!["State answered 404"]);
}

#[test]
fn message_field_is_used_without_failures() {
    let answer = Answer {
        status: 400,
        body: json!({"error": "Bad Request", "message": "Malformed body"}),
    };
    assert_eq!(answer.messages(), vec!["Malformed body"]);
}

#[test]
fn posts_exact_version_body() {
    let state = FakeState::answering(&[(204, "")]);
    HttpState::new(&state.url)
        .validate_version(&version_body())
        .unwrap();
    let requests = state.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/validate/versions");
    let posted: Value = serde_json::from_str(&requests[0].body).unwrap();
    assert_eq!(posted, serde_json::to_value(version_body()).unwrap());
}

#[test]
fn posts_candidate_body_to_candidate_route() {
    let state = FakeState::answering(&[(204, "")]);
    HttpState::new(&state.url)
        .validate_candidate(&candidate_body())
        .unwrap();
    let requests = state.requests();
    assert_eq!(requests[0].path, "/validate/candidates");
    let posted: Value = serde_json::from_str(&requests[0].body).unwrap();
    assert_eq!(posted, serde_json::to_value(candidate_body()).unwrap());
}

#[test]
fn unreachable_state_is_an_error() {
    let state = FakeState::answering(&[]);
    let url = state.url.clone();
    drop(state);
    assert!(
        HttpState::new(&url)
            .validate_version(&version_body())
            .is_err()
    );
}

#[test]
fn empty_state_url_falls_back() {
    assert_eq!(resolve_state_url(Some(String::new())), DEFAULT_STATE_URL);
    assert_eq!(resolve_state_url(None), DEFAULT_STATE_URL);
    assert_eq!(
        resolve_state_url(Some("http://127.0.0.1:1".to_owned())),
        "http://127.0.0.1:1"
    );
}
