mod support;

use sdkman_contrib::publish::{PublishError, RELEASE_PLATFORMS, publish_release};
use sdkman_contrib::state_body::VersionBody;
use support::fakes::FakeVendorState;

const VERSION: &str = "1.1.0";

fn bodies() -> Vec<VersionBody> {
    RELEASE_PLATFORMS
        .iter()
        .map(|release_platform| VersionBody {
            candidate: "contrib".to_owned(),
            version: VERSION.to_owned(),
            platform: release_platform.platform.to_owned(),
            url: format!(
                "https://example.com/contrib-{VERSION}-{}",
                release_platform.suffix
            ),
            md5sum: None,
            sha256sum: "ab".repeat(32),
            sha512sum: None,
            tags: Some(vec!["stable".to_owned()]),
        })
        .collect()
}

fn row(platform: &str) -> (String, String) {
    (VERSION.to_owned(), platform.to_owned())
}

fn calls_of(state: &FakeVendorState, call: &str) -> Vec<String> {
    state
        .calls()
        .into_iter()
        .filter_map(|entry| entry.strip_prefix(&format!("{call} ")).map(str::to_owned))
        .collect()
}

fn failure(state: &FakeVendorState) -> PublishError {
    publish_release(state, &bodies()).unwrap_err()
}

#[test]
fn publishes_and_tags_linux_x64_last() {
    let state = FakeVendorState::default();

    publish_release(&state, &bodies()).unwrap();

    let calls = state.calls();
    let reads: Vec<&str> = calls[..10].iter().map(String::as_str).collect();
    assert_eq!(
        reads,
        [
            "stable_version LINUX_ARM64",
            "has_version LINUX_ARM64",
            "stable_version MAC_X64",
            "has_version MAC_X64",
            "stable_version MAC_ARM64",
            "has_version MAC_ARM64",
            "stable_version WINDOWS_X64",
            "has_version WINDOWS_X64",
            "stable_version LINUX_X64",
            "has_version LINUX_X64",
        ]
    );
    let first_tag = calls
        .iter()
        .position(|call| call.starts_with("tag_stable"))
        .unwrap();
    assert_eq!(first_tag, 15);
    assert_eq!(calls.last().unwrap(), "tag_stable LINUX_X64");
    assert_eq!(state.rows().len(), 5);
    assert!(state.stable().values().all(|version| version == VERSION));
    assert_eq!(state.stable().len(), 5);
}

#[test]
fn failed_tag_restores_the_previous_stable() {
    let state = FakeVendorState::default()
        .with_stable("LINUX_ARM64", "1.0.0")
        .with_stable("MAC_X64", "0.9.0")
        .failing_on("tag_stable", "MAC_ARM64");

    let error = failure(&state);

    assert!(error.cause.contains("tagging MAC_ARM64"), "{error}");
    assert!(error.rollback_failures.is_empty(), "{error}");
    let stable = state.stable();
    assert_eq!(stable.get("LINUX_ARM64").unwrap(), "1.0.0");
    assert_eq!(stable.get("MAC_X64").unwrap(), "0.9.0");
    assert_eq!(stable.len(), 2);
    assert!(state.rows().iter().all(|(version, _)| version != VERSION));
}

#[test]
fn failed_tag_of_linux_x64_restores_every_platform() {
    let state = FakeVendorState::default()
        .with_stable("LINUX_X64", "1.0.0")
        .failing_on("tag_stable", "LINUX_X64");

    failure(&state);

    assert_eq!(state.stable().len(), 1);
    assert_eq!(state.stable().get("LINUX_X64").unwrap(), "1.0.0");
    assert_eq!(
        calls_of(&state, "untag_stable"),
        ["WINDOWS_X64", "MAC_ARM64", "MAC_X64", "LINUX_ARM64"]
    );
}

#[test]
fn failed_post_deletes_new_rows() {
    let state = FakeVendorState::default().failing_on("post_version", "WINDOWS_X64");

    let error = failure(&state);

    assert!(error.cause.contains("posting WINDOWS_X64"), "{error}");
    assert!(state.rows().is_empty());
    assert!(calls_of(&state, "tag_stable").is_empty());
    assert_eq!(
        calls_of(&state, "delete_version"),
        ["LINUX_ARM64", "MAC_X64", "MAC_ARM64"]
    );
}

#[test]
fn rerun_keeps_existing_rows() {
    let state = FakeVendorState::default()
        .with_stable("LINUX_ARM64", VERSION)
        .with_row(VERSION, "MAC_X64")
        .failing_on("tag_stable", "MAC_ARM64");

    failure(&state);

    assert_eq!(
        calls_of(&state, "delete_version"),
        ["MAC_ARM64", "WINDOWS_X64", "LINUX_X64"]
    );
    assert!(state.rows().contains(&row("LINUX_ARM64")));
    assert!(state.rows().contains(&row("MAC_X64")));
    assert_eq!(state.stable().get("LINUX_ARM64").unwrap(), VERSION);
}

#[test]
fn unreadable_state_writes_nothing() {
    let state = FakeVendorState::default().failing_on("has_version", "MAC_ARM64");

    let error = failure(&state);

    assert!(error.cause.contains("MAC_ARM64"), "{error}");
    assert!(calls_of(&state, "post_version").is_empty());
    assert!(calls_of(&state, "delete_version").is_empty());
}

#[test]
fn missing_platform_row_writes_nothing() {
    let state = FakeVendorState::default();
    let mut bodies = bodies();
    bodies.retain(|body| body.platform != "WINDOWS_X64");

    let error = publish_release(&state, &bodies).unwrap_err();

    assert!(error.cause.contains("WINDOWS_X64"), "{error}");
    assert!(calls_of(&state, "post_version").is_empty());
}

#[test]
fn rows_post_without_tags() {
    let state = FakeVendorState::default();

    publish_release(&state, &bodies()).unwrap();

    assert_eq!(state.posted().len(), 5);
    assert!(state.posted().iter().all(|body| body.tags.is_none()));
}
