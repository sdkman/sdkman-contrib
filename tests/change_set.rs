mod support;

use sdkman_contrib::change_set::added_request;
use support::repo::TempRepo;

const REQUEST: &str = "requests/jpx/20261025T120000Z-register-jpx.yaml";

fn repo_with_base() -> (TempRepo, String) {
    let repo = TempRepo::new();
    repo.write(
        "requests/jpx/20261001T000000Z-register-old.yaml",
        "old: true\n",
    );
    repo.write("README.md", "readme\n");
    let base = repo.commit_all("base");
    (repo, base)
}

#[test]
fn accepts_one_added_request() {
    let (repo, base) = repo_with_base();
    repo.write(REQUEST, "new: true\n");
    repo.commit_all("add request");

    assert_eq!(added_request(repo.path(), &base), Ok(REQUEST.to_owned()));
}

#[test]
fn rejects_two_added_files() {
    let (repo, base) = repo_with_base();
    repo.write(REQUEST, "new: true\n");
    repo.write(
        "requests/jpx/20261025T120001Z-release-jpx.yaml",
        "new: true\n",
    );
    repo.commit_all("add two");

    assert_eq!(
        added_request(repo.path(), &base),
        Err("a request PR adds exactly one file under requests/".to_owned())
    );
}

#[test]
fn rejects_edited_request() {
    let (repo, base) = repo_with_base();
    repo.write(
        "requests/jpx/20261001T000000Z-register-old.yaml",
        "old: false\n",
    );
    repo.commit_all("edit request");

    assert_eq!(
        added_request(repo.path(), &base),
        Err("a request PR adds exactly one file under requests/".to_owned())
    );
}

#[test]
fn rejects_request_plus_other_file() {
    let (repo, base) = repo_with_base();
    repo.write(REQUEST, "new: true\n");
    repo.write("README.md", "changed\n");
    repo.commit_all("add request and edit readme");

    assert_eq!(
        added_request(repo.path(), &base),
        Err("a request PR adds exactly one file under requests/".to_owned())
    );
}

#[test]
fn rejects_added_file_outside_requests() {
    let (repo, base) = repo_with_base();
    repo.write("docs/notes.md", "notes\n");
    repo.commit_all("add notes");

    assert_eq!(
        added_request(repo.path(), &base),
        Err("a request PR adds exactly one file under requests/".to_owned())
    );
}

#[test]
fn rejects_applied_path() {
    let (repo, base) = repo_with_base();
    repo.write(REQUEST, "new: true\n");
    repo.write(
        ".applied/jpx/20261025T120000Z-register-jpx.yaml",
        "receipt\n",
    );
    repo.commit_all("add request and receipt");

    assert_eq!(
        added_request(repo.path(), &base),
        Err(
            ".applied/jpx/20261025T120000Z-register-jpx.yaml is under .applied/; only the apply bot writes receipts"
                .to_owned()
        )
    );
}

#[test]
fn reports_unknown_base() {
    let (repo, _) = repo_with_base();

    let error = added_request(repo.path(), "does-not-exist").unwrap_err();

    assert!(
        error.contains("git diff against does-not-exist failed"),
        "{error}"
    );
}
