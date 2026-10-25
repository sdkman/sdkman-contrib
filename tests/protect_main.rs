#![cfg(unix)]

use assert_cmd::Command;
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tempfile::TempDir;

const SCRIPT: &str = include_str!("../scripts/protect-main.sh");

const STUB_GH: &str = r#"#!/usr/bin/env bash
printf '%s\n' "$*" >> "$STUB_DIR/calls.log"
case "$*" in
  *includes_parents*) cat "$STUB_DIR/list.json" ;;
  *--method*) ;;
  *) cat "$STUB_DIR/live.json" ;;
esac
"#;

fn committed() -> Value {
    json!({
        "name": "main",
        "target": "branch",
        "enforcement": "active",
        "conditions": { "ref_name": { "include": ["~DEFAULT_BRANCH"], "exclude": [] } },
        "rules": [
            {
                "type": "pull_request",
                "parameters": { "required_approving_review_count": 1 }
            },
            {
                "type": "required_status_checks",
                "parameters": { "required_status_checks": [{ "context": "pull-request" }] }
            },
            { "type": "non_fast_forward" }
        ],
        "bypass_actors": [
            { "actor_id": 7, "actor_type": "Integration", "bypass_mode": "always" },
            { "actor_id": 5, "actor_type": "RepositoryRole", "bypass_mode": "always" }
        ]
    })
}

fn release_tags() -> Value {
    json!({
        "name": "release-tags",
        "target": "tag",
        "enforcement": "active",
        "conditions": { "ref_name": { "include": ["refs/tags/v*"], "exclude": [] } },
        "rules": [{ "type": "creation" }, { "type": "deletion" }],
        "bypass_actors": [
            { "actor_id": 5, "actor_type": "RepositoryRole", "bypass_mode": "always" }
        ]
    })
}

fn live_copy_of(committed: &Value) -> Value {
    let mut live = committed.clone();
    live["id"] = json!(42);
    live["source_type"] = json!("Repository");
    live
}

struct Sandbox {
    root: TempDir,
}

impl Sandbox {
    fn new(committed: &Value) -> Self {
        let root = TempDir::new().unwrap();
        let scripts = root.path().join("scripts");
        let rulesets = root.path().join(".github/rulesets");
        let stub_bin = root.path().join("bin");
        for dir in [&scripts, &rulesets, &stub_bin] {
            fs::create_dir_all(dir).unwrap();
        }
        fs::write(scripts.join("protect-main.sh"), SCRIPT).unwrap();
        let gh = stub_bin.join("gh");
        fs::write(&gh, STUB_GH).unwrap();
        fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
        let sandbox = Sandbox { root };
        sandbox.add_ruleset("main.json", committed);
        sandbox.live_list(&json!([]));
        sandbox
    }

    fn add_ruleset(&self, file_name: &str, ruleset: &Value) {
        fs::write(
            self.root.path().join(".github/rulesets").join(file_name),
            ruleset.to_string(),
        )
        .unwrap();
    }

    fn stub_dir(&self) -> PathBuf {
        self.root.path().join("stub")
    }

    fn stub_file(&self, name: &str, content: &Value) {
        fs::create_dir_all(self.stub_dir()).unwrap();
        fs::write(self.stub_dir().join(name), content.to_string()).unwrap();
    }

    fn live_list(&self, list: &Value) {
        self.stub_file("list.json", list);
    }

    fn live_ruleset(&self, ruleset: &Value) {
        self.live_list(&json!([{ "id": 42, "name": "main" }, { "id": 1, "name": "other" }]));
        self.stub_file("live.json", ruleset);
    }

    fn run(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        let path = format!(
            "{}:{}",
            self.root.path().join("bin").display(),
            std::env::var("PATH").unwrap()
        );
        Command::new("bash")
            .arg(self.root.path().join("scripts/protect-main.sh"))
            .args(args)
            .env("PATH", path)
            .env("STUB_DIR", self.stub_dir())
            .assert()
    }

    fn gh_calls(&self) -> Vec<String> {
        fs::read_to_string(self.stub_dir().join("calls.log"))
            .map(|log| log.lines().map(str::to_owned).collect())
            .unwrap_or_default()
    }
}

#[test]
fn absent_ruleset_is_created_with_post() {
    let sandbox = Sandbox::new(&committed());

    sandbox.run(&[]).success();

    let calls = sandbox.gh_calls();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(calls[1].contains("--method POST"), "{calls:?}");
    assert!(calls[1].contains("--input"), "{calls:?}");
    assert!(calls[1].ends_with("main.json"), "{calls:?}");
    assert!(
        calls[1].contains("sdkman-contrib/rulesets --input"),
        "{calls:?}"
    );
}

#[test]
fn present_ruleset_is_updated_with_put() {
    let sandbox = Sandbox::new(&committed());
    sandbox.live_ruleset(&live_copy_of(&committed()));

    sandbox.run(&[]).success();

    let calls = sandbox.gh_calls();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(
        calls[1].contains("--method PUT repos/sdkman/sdkman-contrib/rulesets/42"),
        "{calls:?}"
    );
}

#[test]
fn check_passes_on_equal_json() {
    let sandbox = Sandbox::new(&committed());
    sandbox.live_ruleset(&live_copy_of(&committed()));

    sandbox.run(&["--check"]).success();

    assert!(
        sandbox
            .gh_calls()
            .iter()
            .all(|call| !call.contains("--method")),
        "--check must not write"
    );
}

#[test]
fn check_fails_on_differing_rules() {
    let sandbox = Sandbox::new(&committed());
    let mut live = live_copy_of(&committed());
    live["rules"] = json!([{ "type": "non_fast_forward" }]);
    sandbox.live_ruleset(&live);

    sandbox
        .run(&["--check"])
        .code(1)
        .stderr(predicates::str::contains(
            "main differs from the live ruleset",
        ));
}

#[test]
fn check_fails_on_a_differing_parameter_value() {
    let sandbox = Sandbox::new(&committed());
    let mut live = live_copy_of(&committed());
    live["rules"][0]["parameters"]["required_approving_review_count"] = json!(2);
    sandbox.live_ruleset(&live);

    sandbox.run(&["--check"]).code(1);
}

#[test]
fn check_fails_on_an_absent_ruleset() {
    let sandbox = Sandbox::new(&committed());

    sandbox
        .run(&["--check"])
        .code(1)
        .stderr(predicates::str::contains(
            "main is absent from the live repository",
        ));
}

#[test]
fn check_ignores_the_order_of_bypass_actors() {
    let sandbox = Sandbox::new(&committed());
    let mut live = live_copy_of(&committed());
    live["bypass_actors"].as_array_mut().unwrap().reverse();
    live["rules"].as_array_mut().unwrap().reverse();
    sandbox.live_ruleset(&live);

    sandbox.run(&["--check"]).success();
}

#[test]
fn check_ignores_a_parameter_the_committed_rule_lacks() {
    let sandbox = Sandbox::new(&committed());
    let mut live = live_copy_of(&committed());
    live["rules"][0]["parameters"]["allowed_merge_methods"] = json!(["merge", "squash"]);
    sandbox.live_ruleset(&live);

    sandbox.run(&["--check"]).success();
}

#[test]
fn check_ignores_a_nested_parameter_the_committed_rule_lacks() {
    let sandbox = Sandbox::new(&committed());
    let mut live = live_copy_of(&committed());
    live["rules"][1]["parameters"]["required_status_checks"][0]["integration_id"] = json!(15368);
    sandbox.live_ruleset(&live);

    sandbox.run(&["--check"]).success();
}

#[test]
fn check_fails_on_a_differing_nested_parameter() {
    let sandbox = Sandbox::new(&committed());
    let mut live = live_copy_of(&committed());
    live["rules"][1]["parameters"]["required_status_checks"][0]["context"] = json!("other");
    sandbox.live_ruleset(&live);

    sandbox
        .run(&["--check"])
        .code(1)
        .stderr(predicates::str::contains(
            "main differs from the live ruleset",
        ));
}

#[test]
fn check_fails_on_differing_enforcement() {
    let sandbox = Sandbox::new(&committed());
    let mut live = live_copy_of(&committed());
    live["enforcement"] = json!("disabled");
    sandbox.live_ruleset(&live);

    sandbox
        .run(&["--check"])
        .code(1)
        .stderr(predicates::str::contains(
            "main differs from the live ruleset",
        ));
}

#[test]
fn check_fails_on_differing_conditions() {
    let sandbox = Sandbox::new(&committed());
    let mut live = live_copy_of(&committed());
    live["conditions"]["ref_name"]["include"] = json!(["refs/heads/release"]);
    sandbox.live_ruleset(&live);

    sandbox
        .run(&["--check"])
        .code(1)
        .stderr(predicates::str::contains(
            "main differs from the live ruleset",
        ));
}

#[test]
fn check_fails_on_a_missing_bypass_actor() {
    let sandbox = Sandbox::new(&committed());
    let mut live = live_copy_of(&committed());
    live["bypass_actors"].as_array_mut().unwrap().pop();
    sandbox.live_ruleset(&live);

    sandbox
        .run(&["--check"])
        .code(1)
        .stderr(predicates::str::contains(
            "main differs from the live ruleset",
        ));
}

#[test]
fn every_ruleset_file_is_applied() {
    let sandbox = Sandbox::new(&committed());
    sandbox.add_ruleset("release-tags.json", &release_tags());
    sandbox.live_ruleset(&live_copy_of(&committed()));

    sandbox.run(&[]).success();

    let writes: Vec<String> = sandbox
        .gh_calls()
        .into_iter()
        .filter(|call| call.contains("--method"))
        .collect();
    assert_eq!(writes.len(), 2, "{writes:?}");
    assert!(
        writes[0].contains("--method PUT repos/sdkman/sdkman-contrib/rulesets/42"),
        "{writes:?}"
    );
    assert!(writes[0].ends_with("main.json"), "{writes:?}");
    assert!(writes[1].contains("--method POST"), "{writes:?}");
    assert!(writes[1].ends_with("release-tags.json"), "{writes:?}");
}

#[test]
fn check_names_each_drifted_ruleset() {
    let sandbox = Sandbox::new(&committed());
    sandbox.add_ruleset("release-tags.json", &release_tags());
    let mut live = live_copy_of(&committed());
    live["enforcement"] = json!("evaluate");
    sandbox.live_ruleset(&live);

    sandbox
        .run(&["--check"])
        .code(1)
        .stderr(predicates::str::contains(
            "main differs from the live ruleset",
        ))
        .stderr(predicates::str::contains(
            "release-tags is absent from the live repository",
        ));
}

#[test]
fn placeholder_actor_exits_2_before_any_gh_call() {
    let mut ruleset = committed();
    ruleset["bypass_actors"][0]["actor_id"] = json!(0);
    let sandbox = Sandbox::new(&ruleset);

    for args in [&[][..], &["--check"][..]] {
        sandbox
            .run(args)
            .code(2)
            .stderr(predicates::str::contains("main.json"));
    }

    assert!(sandbox.gh_calls().is_empty(), "{:?}", sandbox.gh_calls());
}

#[test]
fn unknown_argument_exits_2_with_usage() {
    let sandbox = Sandbox::new(&committed());

    sandbox
        .run(&["--apply"])
        .code(2)
        .stderr(predicates::str::contains("usage:"));

    assert!(sandbox.gh_calls().is_empty());
}
