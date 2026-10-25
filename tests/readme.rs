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

    let archives = readme
        .split("\n## ")
        .find(|section| section.starts_with("Archives"))
        .expect("README.md has an Archives section");

    assert!(archives.contains("5 redirects"));
    assert!(archives.contains("/releases/latest/"));
    assert!(!archives.contains("answers a redirect"));
}
