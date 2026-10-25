mod support;

use sdkman_contrib::git::Git;
use support::repo::TempRepo;

#[test]
fn added_in_names_the_adding_commit() {
    let repo = TempRepo::new();
    repo.write("requests/jpx/a.yaml", "a\n");
    let added = repo.commit_all("add request");
    repo.write("requests/jpx/a.yaml", "b\n");
    repo.commit_all("edit request");

    let commit = Git::new(repo.path())
        .added_in("requests/jpx/a.yaml")
        .expect("added in");

    assert!(added.starts_with(&commit));
}

#[test]
fn added_in_names_the_merge_commit() {
    let repo = TempRepo::new();
    repo.write("README.md", "hello\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-b", "feature"]);
    repo.write("requests/jpx/a.yaml", "a\n");
    repo.commit_all("add request");
    repo.git(&["checkout", "main"]);
    repo.git(&["merge", "--no-ff", "feature", "-m", "merge feature"]);
    let merge = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();

    let commit = Git::new(repo.path())
        .added_in("requests/jpx/a.yaml")
        .expect("added in");

    assert!(merge.starts_with(&commit), "{commit} is not {merge}");
}

#[test]
fn added_in_full_names_the_whole_sha() {
    let repo = TempRepo::new();
    repo.write("README.md", "hello\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-b", "feature"]);
    repo.write("requests/jpx/a.yaml", "a\n");
    repo.commit_all("add request");
    repo.git(&["checkout", "main"]);
    repo.git(&["merge", "--no-ff", "feature", "-m", "merge feature"]);
    let merge = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();

    let commit = Git::new(repo.path())
        .added_in_full("requests/jpx/a.yaml")
        .expect("added in full");

    assert_eq!(commit, merge);
}

#[test]
fn added_in_fails_for_an_unknown_path() {
    let repo = TempRepo::new();
    repo.write("other.txt", "x\n");
    repo.commit_all("other");

    let error = Git::new(repo.path())
        .added_in("requests/jpx/a.yaml")
        .expect_err("no commit adds it");

    assert_eq!(error.to_string(), "no commit adds requests/jpx/a.yaml");
}

#[test]
fn push_rebases_over_remote_commit() {
    let repo = TempRepo::with_remote();
    let other = repo.clone_remote();
    other.write("other.txt", "first\n");
    other.commit_all("remote commit");
    other.push();

    repo.write(".applied/jpx/a.yaml", "applied_at: now\n");
    Git::new(repo.path())
        .commit_and_push(".applied/jpx/a.yaml", "chore(apply): record a")
        .expect("commit and push");

    let log = repo.remote_log();
    assert!(
        log.starts_with("chore(apply): record a\nremote commit\n"),
        "{log}"
    );
}

#[test]
fn commit_and_push_reports_git_stderr() {
    let repo = TempRepo::new();

    let error = Git::new(repo.path())
        .commit_and_push("missing.yaml", "chore(apply): record")
        .expect_err("nothing to add");

    assert!(error.to_string().starts_with("git add failed:"), "{error}");
}
