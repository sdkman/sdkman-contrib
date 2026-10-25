mod support;

use sdkman_contrib::state_api::{
    Answer, Credentials, DEFAULT_STATE_URL, HttpState, StateValidator, StateWriter, VendorState,
    WriteError, resolve_state_url,
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

#[test]
fn failure_without_message_falls_back_to_status() {
    let state = FakeState::answering(&[(
        400,
        r#"{"error":"Validation Error","failures":[{"field":"url"}]}"#,
    )]);
    let answer = HttpState::new(&state.url)
        .validate_version(&version_body())
        .unwrap();
    assert_eq!(answer.messages(), vec!["State answered 400"]);
}

#[test]
fn trailing_slash_is_trimmed_from_the_base_url() {
    let state = FakeState::answering(&[(204, "")]);
    HttpState::new(&format!("{}/", state.url))
        .validate_version(&version_body())
        .unwrap();
    assert_eq!(state.paths(), vec!["/validate/versions"]);
}

#[test]
fn failed_relogin_after_401_is_a_login_error() {
    let state = FakeState::answering(&[TOKEN_ONE, (401, ""), (401, "")]);
    let error = community(&state.url)
        .post_version(&version_body())
        .unwrap_err();
    assert_eq!(
        error,
        WriteError::Login("login failed: State answered 401".to_owned())
    );
    assert_eq!(state.paths(), vec!["/login", "/versions", "/login"]);
}

#[test]
fn stable_version_reads_the_tag_route() {
    let state = FakeState::answering(&[
        (200, r#"{"candidate":"contrib","version":"0.2.0"}"#),
        (404, ""),
        (500, ""),
    ]);
    let reader = HttpState::new(&state.url);
    assert_eq!(
        reader.stable_version("contrib", "LINUX_X64").unwrap(),
        Some("0.2.0".to_owned())
    );
    assert_eq!(reader.stable_version("contrib", "LINUX_X64").unwrap(), None);
    assert!(matches!(
        reader.stable_version("contrib", "LINUX_X64"),
        Err(WriteError::Refused { status: 500, .. })
    ));
    let requests = state.requests();
    assert_eq!(requests[0].method, "GET");
    assert!(
        requests[0]
            .path
            .starts_with("/versions/contrib/tags/stable?platform=LINUX_X64&_cb=")
    );
}

#[test]
fn has_version_reads_the_version_route() {
    let state = FakeState::answering(&[(200, "{}"), (404, ""), (500, "")]);
    let reader = HttpState::new(&state.url);
    assert!(reader.has_version("contrib", "0.2.0", "MAC_ARM64").unwrap());
    assert!(!reader.has_version("contrib", "0.2.0", "MAC_ARM64").unwrap());
    assert!(matches!(
        reader.has_version("contrib", "0.2.0", "MAC_ARM64"),
        Err(WriteError::Refused { status: 500, .. })
    ));
    assert!(
        state.requests()[0]
            .path
            .starts_with("/versions/contrib/0.2.0?platform=MAC_ARM64&_cb=")
    );
}

#[test]
fn reads_bypass_the_edge_cache() {
    let state = FakeState::answering(&[(404, ""), (404, "")]);
    let reader = HttpState::new(&state.url);
    reader.stable_version("contrib", "LINUX_X64").unwrap();
    reader.stable_version("contrib", "LINUX_X64").unwrap();
    let paths = state.paths();
    assert_ne!(paths[0], paths[1]);
}

#[test]
fn unreachable_read_is_a_transport_error() {
    let state = FakeState::answering(&[]);
    let url = state.url.clone();
    drop(state);
    assert!(matches!(
        HttpState::new(&url).has_version("contrib", "0.2.0", "LINUX_X64"),
        Err(WriteError::Transport(_))
    ));
}

#[test]
fn tag_stable_posts_the_tag() {
    let state = FakeState::answering(&[TOKEN_ONE, (204, "")]);
    community(&state.url)
        .tag_stable("contrib", "0.2.0", "LINUX_X64")
        .unwrap();
    let requests = state.requests();
    assert_eq!(requests[1].method, "POST");
    assert_eq!(requests[1].path, "/versions/tags");
    assert_eq!(requests[1].authorization.as_deref(), Some("Bearer t1"));
    let posted: Value = serde_json::from_str(&requests[1].body).unwrap();
    assert_eq!(
        posted,
        json!({"candidate": "contrib", "version": "0.2.0", "platform": "LINUX_X64", "tag": "stable"})
    );
}

#[test]
fn untag_stable_deletes_the_tag() {
    let state = FakeState::answering(&[TOKEN_ONE, (204, ""), (404, "")]);
    let writer = community(&state.url);
    writer.untag_stable("contrib", "MAC_X64").unwrap();
    writer.untag_stable("contrib", "MAC_X64").unwrap();
    let requests = state.requests();
    assert_eq!(requests[1].method, "DELETE");
    assert_eq!(requests[1].path, "/versions/tags");
    assert_eq!(requests[1].authorization.as_deref(), Some("Bearer t1"));
    let sent: Value = serde_json::from_str(&requests[1].body).unwrap();
    assert_eq!(
        sent,
        json!({"candidate": "contrib", "platform": "MAC_X64", "tag": "stable"})
    );
}

#[test]
fn delete_version_sends_a_null_distribution() {
    let state = FakeState::answering(&[TOKEN_ONE, (204, ""), (404, "")]);
    let writer = community(&state.url);
    writer
        .delete_version("contrib", "0.2.0", "MAC_ARM64")
        .unwrap();
    writer
        .delete_version("contrib", "0.2.0", "MAC_ARM64")
        .unwrap();
    let requests = state.requests();
    assert_eq!(requests[1].method, "DELETE");
    assert_eq!(requests[1].path, "/versions");
    let sent: Value = serde_json::from_str(&requests[1].body).unwrap();
    assert_eq!(
        sent,
        json!({
            "candidate": "contrib",
            "version": "0.2.0",
            "platform": "MAC_ARM64",
            "distribution": null
        })
    );
}

#[test]
fn tagged_row_delete_is_refused() {
    let state = FakeState::answering(&[
        TOKEN_ONE,
        (409, r#"{"error":"Conflict","message":"version is tagged"}"#),
    ]);
    let error = community(&state.url)
        .delete_version("contrib", "0.2.0", "LINUX_X64")
        .unwrap_err();
    assert_eq!(
        error,
        WriteError::Refused {
            status: 409,
            messages: vec!["version is tagged".to_owned()]
        }
    );
}

#[test]
fn delete_relogs_in_after_401() {
    let state = FakeState::answering(&[TOKEN_ONE, (401, ""), TOKEN_TWO, (204, "")]);
    community(&state.url)
        .untag_stable("contrib", "LINUX_X64")
        .unwrap();
    let requests = state.requests();
    assert_eq!(
        state.paths(),
        vec!["/login", "/versions/tags", "/login", "/versions/tags"]
    );
    assert_eq!(requests[3].method, "DELETE");
    assert_eq!(requests[3].authorization.as_deref(), Some("Bearer t2"));
}
