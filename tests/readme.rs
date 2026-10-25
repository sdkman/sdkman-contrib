use sdkman_contrib::request::{Request, parse_request};

fn candidate_blocks(readme: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current: Option<Vec<&str>> = None;
    for line in readme.lines() {
        match (&mut current, line.trim_end()) {
            (None, "```yaml") => current = Some(Vec::new()),
            (None, _) => {}
            (Some(lines), "```") => {
                if lines.first().is_some_and(|l| l.starts_with("candidate:")) {
                    blocks.push(lines.join("\n") + "\n");
                }
                current = None;
            }
            (Some(lines), _) => lines.push(line),
        }
    }
    blocks
}

/// From `heading` up to the next heading of the same or a higher level.
fn readme_section<'a>(readme: &'a str, heading: &str) -> &'a str {
    let level = |line: &str| line.len() - line.trim_start_matches('#').len();
    let start = readme
        .find(&format!("\n{heading}\n"))
        .unwrap_or_else(|| panic!("README.md has a {heading} section"))
        + 1;
    let section = &readme[start..];
    let end = section
        .match_indices("\n#")
        .map(|(index, _)| index)
        .find(|&index| level(&section[index + 1..]) <= level(heading))
        .unwrap_or(section.len());
    &section[..end]
}

#[test]
fn readme_examples_parse() {
    let readme = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))
        .expect("README.md is readable");

    let requests: Vec<Request> = candidate_blocks(&readme)
        .iter()
        .map(|block| parse_request(block).unwrap_or_else(|e| panic!("{e}\n{block}")))
        .collect();

    let registrations = requests
        .iter()
        .filter(|r| matches!(r, Request::Registration(_)))
        .count();
    let releases: Vec<_> = requests
        .iter()
        .filter_map(|r| match r {
            Request::Release(release) => Some(release),
            Request::Registration(_) => None,
        })
        .collect();

    assert_eq!(registrations, 1);
    assert_eq!(releases.len(), 2);
    assert_eq!(releases.iter().filter(|r| r.default.is_some()).count(), 1);
    for release in releases {
        assert!(release.structure_errors().is_empty());
    }
}

#[test]
fn readme_states_the_archive_rules() {
    let readme = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))
        .expect("README.md is readable");

    let archives = readme_section(&readme, "## Archives");

    assert!(archives.contains("5 redirects"));
    assert!(archives.contains("/releases/latest/"));
    assert!(!archives.contains("answers a redirect"));
}

#[test]
fn readme_states_the_review_rules() {
    let readme = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))
        .expect("README.md is readable");

    assert!(readme_section(&readme, "### Checksums").contains("byte for byte"));
    assert!(readme_section(&readme, "## The check on your PR").contains("never code from your PR"));
    assert!(!readme.contains("only thing that pins"));
}

#[test]
fn readme_names_the_install_path() {
    let readme = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))
        .expect("README.md is readable");

    let install = readme_section(&readme, "## Installing contrib");

    assert!(readme.find("## Installing contrib") < readme.find("## How it works"));
    for literal in [
        "sdk update",
        "new terminal",
        "sdk install contrib",
        "sdk upgrade contrib",
        "Linux x64",
        "Linux ARM64",
        "macOS x64",
        "macOS ARM64",
        "Windows x64",
        "contrib --version",
        "contrib <version>",
        "GitHub release",
        "cargo install --git https://github.com/sdkman/sdkman-contrib",
    ] {
        assert!(install.contains(literal), "missing {literal}");
    }
    assert!(!readme.contains("You never have to install it"));
}

#[test]
fn version_prints_contrib_and_the_cargo_version() {
    assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("contrib"))
        .arg("--version")
        .assert()
        .success()
        .stdout(format!("contrib {}\n", env!("CARGO_PKG_VERSION")));
}
