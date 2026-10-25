mod support;

use sdkman_contrib::state_api::{
    Answer, Credentials, DEFAULT_STATE_URL, HttpState, StateValidator, StateWriter, WriteError,
    resolve_state_url,
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

const TOKEN_ONE: (u16, &str) = (200, r#"{"token":"t1"}"#);
const TOKEN_TWO: (u16, &str) = (200, r#"{"token":"t2"}"#);

fn community(url: &str) -> HttpState {
    HttpState::with_credentials(
        url,
        Credentials {
            email: "community@sdkman.io".to_owned(),
            password: "secret".to_owned(),
        },
    )
}

#[test]
fn login_posts_credentials() {
    let state = FakeState::answering(&[TOKEN_ONE]);
    community(&state.url).login().unwrap();
    let requests = state.requests();
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/login");
    let posted: Value = serde_json::from_str(&requests[0].body).unwrap();
    assert_eq!(
        posted,
        json!({"email": "community@sdkman.io", "password": "secret"})
    );
}

#[test]
fn login_failure_is_an_error() {
    let state = FakeState::answering(&[(401, "")]);
    let error = community(&state.url).login().unwrap_err();
    assert_eq!(error, "login failed: State answered 401");
}

#[test]
fn login_without_token_is_an_error() {
    let state = FakeState::answering(&[(200, "{}")]);
    assert!(community(&state.url).login().is_err());
}

#[test]
fn write_without_credentials_never_reaches_state() {
    let state = FakeState::answering(&[]);
    let error = HttpState::new(&state.url)
        .post_version(&version_body())
        .unwrap_err();
    assert!(matches!(error, WriteError::Login(_)));
    assert!(state.requests().is_empty());
}

#[test]
fn version_write_logs_in_first_and_sends_bearer() {
    let state = FakeState::answering(&[TOKEN_ONE, (204, "")]);
    community(&state.url).post_version(&version_body()).unwrap();
    let requests = state.requests();
    assert_eq!(state.paths(), vec!["/login", "/versions"]);
    assert_eq!(requests[1].authorization.as_deref(), Some("Bearer t1"));
    let posted: Value = serde_json::from_str(&requests[1].body).unwrap();
    assert_eq!(posted, serde_json::to_value(version_body()).unwrap());
}

#[test]
fn candidate_write_accepts_200_and_201_on_one_login() {
    let state = FakeState::answering(&[TOKEN_ONE, (201, ""), (200, "")]);
    let writer = community(&state.url);
    writer.post_candidate(&candidate_body()).unwrap();
    writer.post_candidate(&candidate_body()).unwrap();
    assert_eq!(
        state.paths(),
        vec!["/login", "/admin/candidates", "/admin/candidates"]
    );
}

#[test]
fn version_write_refuses_anything_but_204() {
    let state = FakeState::answering(&[TOKEN_ONE, (200, "")]);
    let error = community(&state.url)
        .post_version(&version_body())
        .unwrap_err();
    assert_eq!(
        error,
        WriteError::Refused {
            status: 200,
            messages: vec!["State answered 200".to_owned()]
        }
    );
}

#[test]
fn refusal_carries_failure_messages() {
    let body = r#"{"error":"Validation Error","failures":[{"field":"url","message":"bad"}]}"#;
    let state = FakeState::answering(&[TOKEN_ONE, (400, body)]);
    let error = community(&state.url)
        .post_version(&version_body())
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "State refused the write with 400: url: bad"
    );
}

#[test]
fn relogs_in_once_after_401() {
    let state = FakeState::answering(&[TOKEN_ONE, (401, ""), TOKEN_TWO, (204, "")]);
    community(&state.url).post_version(&version_body()).unwrap();
    let requests = state.requests();
    assert_eq!(
        state.paths(),
        vec!["/login", "/versions", "/login", "/versions"]
    );
    assert_eq!(requests[3].authorization.as_deref(), Some("Bearer t2"));
}

#[test]
fn second_401_is_refused_without_a_third_login() {
    let state = FakeState::answering(&[TOKEN_ONE, (401, ""), TOKEN_TWO, (401, "")]);
    let error = community(&state.url)
        .post_version(&version_body())
        .unwrap_err();
    assert!(matches!(error, WriteError::Refused { status: 401, .. }));
    assert_eq!(state.requests().len(), 4);
}
