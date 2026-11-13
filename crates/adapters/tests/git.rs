use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use coldstart_adapters::{GitAdapter, GitAuth, GitError, GitHubToken};
use tempfile::TempDir;

struct Fixture {
    _directory: TempDir,
    remote: PathBuf,
    work: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("temporary directory must exist");
        let remote = directory.path().join("remote.git");
        let work = directory.path().join("work");
        git(
            directory.path(),
            &[
                "init",
                "--bare",
                remote.to_str().expect("path must be UTF-8"),
            ],
        );
        fs::create_dir_all(&work).expect("work directory must create");
        git(&work, &["init", "-b", "main"]);
        git(&work, &["config", "user.name", "ColdStart Test"]);
        git(
            &work,
            &["config", "user.email", "coldstart@example.invalid"],
        );
        fs::write(work.join("README.md"), "initial\n").expect("initial file must write");
        git(&work, &["add", "README.md"]);
        git(&work, &["commit", "-m", "initial"]);
        git(
            &work,
            &[
                "remote",
                "add",
                "origin",
                remote.to_str().expect("path must be UTF-8"),
            ],
        );
        git(&work, &["push", "--set-upstream", "origin", "main"]);
        Self {
            _directory: directory,
            remote,
            work,
        }
    }

    fn write_and_commit(&self, relative_path: &str, content: &str, message: &str) -> String {
        let path = self.work.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent directory must create");
        }
        fs::write(path, content).expect("file must write");
        git(&self.work, &["add", relative_path]);
        git(&self.work, &["commit", "-m", message]);
        git_output(&self.work, &["rev-parse", "HEAD"])
    }
}

fn git(path: &Path, args: &[&str]) -> std::process::Output {
    let output = Command::new("git")
        .current_dir(path)
        .args(args)
        .output()
        .expect("git must run");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn git_output(path: &Path, args: &[&str]) -> String {
    String::from_utf8(git(path, args).stdout)
        .expect("git output must be UTF-8")
        .trim()
        .to_owned()
}

#[test]
fn inspects_and_validates_a_clean_repository() {
    let fixture = Fixture::new();
    let adapter = GitAdapter::new();
    let status = adapter
        .inspect(&fixture.work)
        .expect("repository must inspect");
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert!(status.clean);
    assert!(status.head_sha.is_some());
    assert!(status.changed_paths.is_empty());
    adapter
        .validate_remote(
            &fixture.work,
            "origin",
            fixture.remote.to_str().expect("path must be UTF-8"),
        )
        .expect("remote must match");
    adapter
        .ensure_clean(&fixture.work)
        .expect("working tree must be clean");
    adapter
        .fetch(&fixture.work, &GitAuth::None)
        .expect("fetch must work");
}

#[test]
fn rejects_dirty_working_trees_and_existing_branches() {
    let fixture = Fixture::new();
    let adapter = GitAdapter::new();
    adapter
        .create_work_branch(
            &fixture.work,
            "main",
            "codex/task-one",
            &["main".to_owned()],
            &GitAuth::None,
        )
        .expect("branch must create");
    let error = adapter
        .create_work_branch(
            &fixture.work,
            "main",
            "codex/task-one",
            &["main".to_owned()],
            &GitAuth::None,
        )
        .expect_err("existing branch must fail");
    assert!(matches!(error, GitError::BranchExists(branch) if branch == "codex/task-one"));

    fs::write(fixture.work.join("README.md"), "dirty\n").expect("file must modify");
    let error = adapter
        .ensure_clean(&fixture.work)
        .expect_err("dirty worktree must fail");
    assert!(matches!(error, GitError::DirtyWorkingTree(_)));
}

#[test]
fn verifies_the_branch_head_and_changed_file_scope() {
    let fixture = Fixture::new();
    let adapter = GitAdapter::new();
    adapter
        .create_work_branch(
            &fixture.work,
            "main",
            "codex/task-one",
            &["main".to_owned()],
            &GitAuth::None,
        )
        .expect("branch must create");
    let commit = fixture.write_and_commit("src/main.rs", "fn main() {}\n", "implement task");
    let verification = adapter
        .verify_report_commit(
            &fixture.work,
            "codex/task-one",
            "main",
            &commit,
            &["src".to_owned()],
            &["main".to_owned()],
        )
        .expect("commit must verify");
    assert_eq!(verification.commit_sha, commit);
    assert_eq!(verification.branch, "codex/task-one");
    assert_eq!(verification.changed_files, ["src/main.rs"]);

    let out_of_scope = fixture.write_and_commit("docs/guide.md", "guide\n", "write docs");
    let error = adapter
        .verify_report_commit(
            &fixture.work,
            "codex/task-one",
            "main",
            &out_of_scope,
            &["src".to_owned()],
            &["main".to_owned()],
        )
        .expect_err("outside path must fail");
    assert!(matches!(error, GitError::ChangesOutsideAllowedPaths(_)));
}

#[test]
fn dot_allowed_path_means_the_entire_repository() {
    let fixture = Fixture::new();
    let adapter = GitAdapter::new();
    adapter
        .create_work_branch(
            &fixture.work,
            "main",
            "codex/task-all-paths",
            &["main".to_owned()],
            &GitAuth::None,
        )
        .expect("branch must create");
    let commit = fixture.write_and_commit("docs/guide.md", "guide\n", "write docs");
    let verification = adapter
        .verify_report_commit(
            &fixture.work,
            "codex/task-all-paths",
            "main",
            &commit,
            &[".".to_owned()],
            &["main".to_owned()],
        )
        .expect("repository root scope must allow nested paths");
    assert_eq!(verification.changed_files, ["docs/guide.md"]);
}

#[test]
fn rejects_commit_that_is_not_the_branch_head() {
    let fixture = Fixture::new();
    let adapter = GitAdapter::new();
    adapter
        .create_work_branch(
            &fixture.work,
            "main",
            "codex/task-one",
            &["main".to_owned()],
            &GitAuth::None,
        )
        .expect("branch must create");
    let first = fixture.write_and_commit("src/one.rs", "one\n", "one");
    fixture.write_and_commit("src/two.rs", "two\n", "two");
    let error = adapter
        .verify_report_commit(
            &fixture.work,
            "codex/task-one",
            "main",
            &first,
            &["src".to_owned()],
            &["main".to_owned()],
        )
        .expect_err("stale commit must fail");
    assert!(matches!(error, GitError::CommitNotBranchHead { .. }));
}

#[test]
fn pushes_a_work_branch_and_refuses_protected_branches() {
    let fixture = Fixture::new();
    let adapter = GitAdapter::new();
    adapter
        .create_work_branch(
            &fixture.work,
            "main",
            "codex/task-one",
            &["main".to_owned()],
            &GitAuth::None,
        )
        .expect("branch must create");
    fixture.write_and_commit("src/main.rs", "fn main() {}\n", "implement task");
    adapter
        .push_branch(
            &fixture.work,
            "codex/task-one",
            &["main".to_owned()],
            &GitAuth::None,
        )
        .expect("branch must push");
    let remote_branch = git_output(&fixture.remote, &["rev-parse", "refs/heads/codex/task-one"]);
    let local_branch = git_output(&fixture.work, &["rev-parse", "codex/task-one"]);
    assert_eq!(remote_branch, local_branch);

    let error = adapter
        .push_branch(&fixture.work, "main", &["main".to_owned()], &GitAuth::None)
        .expect_err("protected branch must fail");
    assert!(matches!(error, GitError::ProtectedBranch(branch) if branch == "main"));
}

#[test]
fn git_failures_never_expose_the_authentication_token() {
    let fixture = Fixture::new();
    let adapter = GitAdapter::new();
    adapter
        .create_work_branch(
            &fixture.work,
            "main",
            "codex/task-one",
            &["main".to_owned()],
            &GitAuth::None,
        )
        .expect("branch must create");
    git(
        &fixture.work,
        &[
            "remote",
            "set-url",
            "origin",
            "http://127.0.0.1:1/private.git",
        ],
    );
    let token = GitHubToken::new("gho_git_failure_secret").expect("token must be valid");
    let auth = GitAuth::github(token);
    assert!(!format!("{auth:?}").contains("gho_git_failure_secret"));
    let error = adapter
        .push_branch(&fixture.work, "codex/task-one", &["main".to_owned()], &auth)
        .expect_err("unreachable remote must fail");
    let message = error.to_string();
    assert!(
        !message.contains("gho_git_failure_secret"),
        "git error leaked token: {message}"
    );
}

#[test]
fn verification_checkout_requires_the_expected_branch_head_and_tracked_cleanliness() {
    let fixture = Fixture::new();
    let adapter = GitAdapter::new();
    adapter
        .create_work_branch(
            &fixture.work,
            "main",
            "codex/verify-me",
            &["main".to_owned()],
            &GitAuth::None,
        )
        .expect("branch must create");
    let commit = fixture.write_and_commit("src/main.rs", "fn main() {}\n", "verification task");
    adapter
        .ensure_verification_checkout(&fixture.work, "codex/verify-me", &commit)
        .expect("clean expected checkout must pass");

    fs::write(fixture.work.join("untracked.log"), "build output\n")
        .expect("untracked file must write");
    adapter
        .ensure_verification_checkout(&fixture.work, "codex/verify-me", &commit)
        .expect("untracked build output must not fail");
    fs::remove_file(fixture.work.join("untracked.log")).expect("untracked file must remove");

    fs::write(
        fixture.work.join("src/main.rs"),
        "fn main() { changed(); }\n",
    )
    .expect("tracked file must modify");
    let error = adapter
        .ensure_verification_checkout(&fixture.work, "codex/verify-me", &commit)
        .expect_err("tracked modification must fail");
    assert!(matches!(error, GitError::VerificationTrackedChanges(_)));
    git(&fixture.work, &["restore", "src/main.rs"]);

    let replacement =
        fixture.write_and_commit("src/main.rs", "fn main() { newer(); }\n", "rewrite HEAD");
    let error = adapter
        .ensure_verification_checkout(&fixture.work, "codex/verify-me", &commit)
        .expect_err("HEAD rewrite must fail");
    assert!(matches!(error, GitError::VerificationHeadMismatch { .. }));
    adapter
        .ensure_verification_checkout(&fixture.work, "codex/verify-me", &replacement)
        .expect("new HEAD must pass");

    git(&fixture.work, &["switch", "main"]);
    let error = adapter
        .ensure_verification_checkout(&fixture.work, "codex/verify-me", &replacement)
        .expect_err("branch switch must fail");
    assert!(matches!(error, GitError::VerificationBranchMismatch { .. }));
}

#[test]
fn locates_conflict_paths_without_running_repository_merge_drivers() {
    let fixture = Fixture::new();
    let adapter = GitAdapter::new();

    fs::write(fixture.work.join(".gitattributes"), "*.txt merge=evil\n")
        .expect("attributes must write");
    git(&fixture.work, &["add", ".gitattributes"]);
    git(&fixture.work, &["commit", "-m", "configure merge driver"]);
    git(&fixture.work, &["switch", "-c", "feature"]);
    let head = fixture.write_and_commit("conflict.txt", "feature\n", "feature change");
    git(&fixture.work, &["switch", "main"]);
    let base = fixture.write_and_commit("conflict.txt", "main\n", "main change");

    let marker = fixture.work.join("driver-ran.txt");
    let driver = format!(
        "powershell -NoProfile -Command \"Set-Content -LiteralPath '{}' -Value ran\"",
        marker.display()
    );
    git(&fixture.work, &["config", "Merge.Evil.Driver", &driver]);

    let report = adapter
        .find_merge_conflicts(&fixture.work, &base, &head, 100)
        .expect("merge conflicts must diagnose");
    assert!(report.has_conflicts);
    assert_eq!(report.conflicted_paths, ["conflict.txt"]);
    assert!(!report.truncated);
    assert!(
        !marker.exists(),
        "repository merge driver must not execute during read-only diagnostics"
    );
    assert_eq!(
        git_output(&fixture.work, &["branch", "--show-current"]),
        "main"
    );
    assert_eq!(git_output(&fixture.work, &["status", "--porcelain"]), "");
}

#[test]
fn reports_clean_merges_and_missing_local_commits() {
    let fixture = Fixture::new();
    let adapter = GitAdapter::new();

    git(&fixture.work, &["switch", "-c", "feature"]);
    let head = fixture.write_and_commit("feature.txt", "feature\n", "feature change");
    git(&fixture.work, &["switch", "main"]);
    let base = fixture.write_and_commit("main.txt", "main\n", "main change");

    let report = adapter
        .find_merge_conflicts(&fixture.work, &base, &head, 100)
        .expect("clean merge must diagnose");
    assert!(!report.has_conflicts);
    assert!(report.conflicted_paths.is_empty());
    assert!(!report.truncated);

    let missing = "a".repeat(40);
    let error = adapter
        .find_merge_conflicts(&fixture.work, &missing, &head, 100)
        .expect_err("missing commit must fail");
    assert!(matches!(error, GitError::CommitNotFound(commit) if commit == missing));
}
