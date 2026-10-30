use std::{
    fmt, io,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use coldstart_core::validate_branch_name;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::GitHubToken;

const MAX_GIT_ERROR_CHARS: usize = 1_200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitRepositoryStatus {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub head_sha: Option<String>,
    pub clean: bool,
    pub changed_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommitVerification {
    pub commit_sha: String,
    pub branch: String,
    pub changed_files: Vec<String>,
}

#[derive(Clone)]
pub enum GitAuth {
    None,
    GitHub(GitHubToken),
    SshAgent,
}

impl GitAuth {
    pub fn github(token: GitHubToken) -> Self {
        Self::GitHub(token)
    }
}

impl fmt::Debug for GitAuth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => formatter.write_str("GitAuth::None"),
            Self::GitHub(_) => formatter.write_str("GitAuth::GitHub(***)"),
            Self::SshAgent => formatter.write_str("GitAuth::SshAgent"),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GitAdapter;

impl GitAdapter {
    pub fn new() -> Self {
        Self
    }

    pub fn inspect(&self, repository: &Path) -> Result<GitRepositoryStatus, GitError> {
        let inside = self.run(
            repository,
            &["rev-parse", "--is-inside-work-tree"],
            "inspect",
        )?;
        if !output_text(&inside).eq_ignore_ascii_case("true") {
            return Err(GitError::NotRepository(repository.to_path_buf()));
        }

        let branch = self
            .run_allow_failure(
                repository,
                &["symbolic-ref", "--quiet", "--short", "HEAD"],
                "inspect branch",
            )?
            .filter(|output| output.status.success())
            .map(|output| output_text(&output))
            .filter(|branch| !branch.is_empty());
        let head_sha = self
            .run_allow_failure(
                repository,
                &["rev-parse", "--verify", "HEAD"],
                "inspect HEAD",
            )?
            .filter(|output| output.status.success())
            .map(|output| output_text(&output))
            .filter(|sha| !sha.is_empty());
        let status = self.run(
            repository,
            &["status", "--porcelain=v1", "--untracked-files=all"],
            "inspect status",
        )?;
        let changed_paths = parse_status_paths(&output_text(&status));
        Ok(GitRepositoryStatus {
            path: repository.to_path_buf(),
            branch,
            head_sha,
            clean: changed_paths.is_empty(),
            changed_paths,
        })
    }

    pub fn validate_remote(
        &self,
        repository: &Path,
        remote_name: &str,
        expected_url: &str,
    ) -> Result<(), GitError> {
        validate_remote_name(remote_name)?;
        let output = self.run(
            repository,
            &["remote", "get-url", remote_name],
            "read remote",
        )?;
        let actual = output_text(&output);
        if normalize_remote(&actual) != normalize_remote(expected_url) {
            return Err(GitError::RemoteMismatch {
                expected: expected_url.to_owned(),
                actual,
            });
        }
        Ok(())
    }

    pub fn set_remote(
        &self,
        repository: &Path,
        remote_name: &str,
        remote_url: &str,
    ) -> Result<(), GitError> {
        validate_remote_name(remote_name)?;
        validate_remote_url(remote_url)?;
        self.run(
            repository,
            &["remote", "set-url", remote_name, remote_url],
            "set remote",
        )?;
        Ok(())
    }

    pub fn fetch(&self, repository: &Path, auth: &GitAuth) -> Result<(), GitError> {
        self.run_with_auth(repository, &["fetch", "origin", "--prune"], auth, "fetch")?;
        Ok(())
    }

    pub fn ensure_clean(&self, repository: &Path) -> Result<(), GitError> {
        let status = self.inspect(repository)?;
        if status.clean {
            Ok(())
        } else {
            Err(GitError::DirtyWorkingTree(status.changed_paths))
        }
    }

    pub fn create_work_branch(
        &self,
        repository: &Path,
        base_ref: &str,
        branch: &str,
        protected_branches: &[String],
        auth: &GitAuth,
    ) -> Result<(), GitError> {
        validate_work_branch(branch, protected_branches)?;
        validate_base_ref(base_ref)?;
        self.ensure_clean(repository)?;
        if self.branch_exists(repository, branch)? {
            return Err(GitError::BranchExists(branch.to_owned()));
        }
        let args = ["switch", "--create", branch, "--no-track", base_ref];
        self.run_with_auth(repository, &args, auth, "create work branch")?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn verify_report_commit(
        &self,
        repository: &Path,
        branch: &str,
        base_ref: &str,
        commit_sha: &str,
        allowed_paths: &[String],
        protected_branches: &[String],
    ) -> Result<GitCommitVerification, GitError> {
        validate_work_branch(branch, protected_branches)?;
        validate_base_ref(base_ref)?;
        validate_commit_sha(commit_sha)?;
        let resolved_commit = self
            .run_allow_failure(
                repository,
                &["rev-parse", "--verify", &format!("{commit_sha}^{{commit}}")],
                "verify commit",
            )?
            .filter(|output| output.status.success())
            .map(|output| output_text(&output))
            .ok_or_else(|| GitError::CommitNotFound(commit_sha.to_owned()))?;
        let branch_ref = format!("refs/heads/{branch}");
        let branch_head = self
            .run_allow_failure(
                repository,
                &["rev-parse", "--verify", &format!("{branch_ref}^{{commit}}")],
                "verify branch",
            )?
            .filter(|output| output.status.success())
            .map(|output| output_text(&output))
            .ok_or_else(|| GitError::BranchNotFound(branch.to_owned()))?;
        if branch_head != resolved_commit {
            return Err(GitError::CommitNotBranchHead {
                branch: branch.to_owned(),
                expected: branch_head,
                actual: resolved_commit,
            });
        }

        let base = self
            .run_allow_failure(
                repository,
                &["rev-parse", "--verify", &format!("{base_ref}^{{commit}}")],
                "verify base ref",
            )?
            .filter(|output| output.status.success())
            .map(|output| output_text(&output))
            .ok_or_else(|| GitError::BaseRefNotFound(base_ref.to_owned()))?;
        let range = format!("{base}...{resolved_commit}");
        let diff = self.run(
            repository,
            &["diff", "--name-only", &range, "--"],
            "read changed files",
        )?;
        let changed_files = output_text(&diff)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let outside = changed_files
            .iter()
            .filter(|path| !path_is_allowed(path, allowed_paths))
            .cloned()
            .collect::<Vec<_>>();
        if !outside.is_empty() {
            return Err(GitError::ChangesOutsideAllowedPaths(outside));
        }
        Ok(GitCommitVerification {
            commit_sha: resolved_commit,
            branch: branch.to_owned(),
            changed_files,
        })
    }

    pub fn push_branch(
        &self,
        repository: &Path,
        branch: &str,
        protected_branches: &[String],
        auth: &GitAuth,
    ) -> Result<(), GitError> {
        validate_work_branch(branch, protected_branches)?;
        let args = ["push", "--set-upstream", "origin", branch];
        self.run_with_auth(repository, &args, auth, "push branch")?;
        Ok(())
    }

    fn branch_exists(&self, repository: &Path, branch: &str) -> Result<bool, GitError> {
        let branch_ref = format!("refs/heads/{branch}");
        let output = self.run_allow_failure(
            repository,
            &["show-ref", "--verify", "--quiet", &branch_ref],
            "check branch",
        )?;
        match output {
            Some(output) if output.status.success() => Ok(true),
            Some(output) if output.status.code() == Some(1) => Ok(false),
            Some(output) => Err(self.command_failed(
                "check branch",
                String::from_utf8_lossy(&output.stderr).as_ref(),
                &GitAuth::None,
            )),
            None => Ok(false),
        }
    }

    fn run(
        &self,
        repository: &Path,
        args: &[&str],
        operation: &'static str,
    ) -> Result<Output, GitError> {
        self.run_with_auth(repository, args, &GitAuth::None, operation)
    }

    fn run_with_auth(
        &self,
        repository: &Path,
        args: &[&str],
        auth: &GitAuth,
        operation: &'static str,
    ) -> Result<Output, GitError> {
        let output = self
            .run_allow_failure_with_auth(repository, args, auth)?
            .ok_or_else(|| GitError::CommandFailed {
                operation,
                stderr: "Git did not produce output".to_owned(),
            })?;
        if output.status.success() {
            Ok(output)
        } else {
            Err(self.command_failed(
                operation,
                String::from_utf8_lossy(&output.stderr).as_ref(),
                auth,
            ))
        }
    }

    fn run_allow_failure(
        &self,
        repository: &Path,
        args: &[&str],
        _operation: &'static str,
    ) -> Result<Option<Output>, GitError> {
        self.run_allow_failure_with_auth(repository, args, &GitAuth::None)
    }

    fn run_allow_failure_with_auth(
        &self,
        repository: &Path,
        args: &[&str],
        auth: &GitAuth,
    ) -> Result<Option<Output>, GitError> {
        let mut command = Command::new("git");
        command
            .current_dir(repository)
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("LC_ALL", "C")
            .env("LANG", "C");
        apply_auth(&mut command, auth);
        match command.output() {
            Ok(output) => Ok(Some(output)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Err(GitError::GitUnavailable),
            Err(error) => Err(GitError::Io(redact_error(&error.to_string(), auth))),
        }
    }

    fn command_failed(&self, operation: &'static str, stderr: &str, auth: &GitAuth) -> GitError {
        GitError::CommandFailed {
            operation,
            stderr: truncate_git_error(&redact_error(stderr, auth)),
        }
    }
}

fn apply_auth(command: &mut Command, auth: &GitAuth) {
    if let GitAuth::GitHub(token) = auth {
        let credential = BASE64.encode(format!("x-access-token:{}", token.expose_secret()));
        command
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "http.https://github.com/.extraheader")
            .env(
                "GIT_CONFIG_VALUE_0",
                format!("AUTHORIZATION: basic {credential}"),
            );
    }
}

fn redact_error(value: &str, auth: &GitAuth) -> String {
    match auth {
        GitAuth::GitHub(token) => {
            let token = token.expose_secret();
            let credential = BASE64.encode(format!("x-access-token:{token}"));
            value.replace(token, "***").replace(&credential, "***")
        }
        GitAuth::None | GitAuth::SshAgent => value.to_owned(),
    }
}

fn truncate_git_error(value: &str) -> String {
    value.chars().take(MAX_GIT_ERROR_CHARS).collect()
}

fn output_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn parse_status_paths(status: &str) -> Vec<String> {
    status
        .lines()
        .filter(|line| line.len() > 3)
        .map(|line| {
            let path = &line[3..];
            path.rsplit(" -> ").next().unwrap_or(path).trim().to_owned()
        })
        .collect()
}

fn normalize_remote(value: &str) -> String {
    let value = value.trim().replace('\\', "/");
    let value = value.trim_end_matches('/');
    value.strip_suffix(".git").unwrap_or(value).to_owned()
}

fn validate_remote_name(value: &str) -> Result<(), GitError> {
    if value.trim().is_empty() || value.starts_with('-') {
        Err(GitError::InvalidRemoteName(value.to_owned()))
    } else {
        Ok(())
    }
}

fn validate_remote_url(value: &str) -> Result<(), GitError> {
    if value.trim().is_empty()
        || value.starts_with('-')
        || value.chars().any(|character| character.is_control())
    {
        Err(GitError::InvalidRemoteUrl(value.to_owned()))
    } else {
        Ok(())
    }
}

fn validate_work_branch(branch: &str, protected_branches: &[String]) -> Result<(), GitError> {
    validate_branch_name(branch).map_err(|_| GitError::InvalidBranchName(branch.to_owned()))?;
    if protected_branches
        .iter()
        .any(|protected| protected == branch)
    {
        return Err(GitError::ProtectedBranch(branch.to_owned()));
    }
    Ok(())
}

fn validate_base_ref(value: &str) -> Result<(), GitError> {
    if validate_branch_name(value).is_ok() || is_hex_sha(value) {
        Ok(())
    } else {
        Err(GitError::InvalidBaseRef(value.to_owned()))
    }
}

fn validate_commit_sha(value: &str) -> Result<(), GitError> {
    if is_hex_sha(value) {
        Ok(())
    } else {
        Err(GitError::CommitNotFound(value.to_owned()))
    }
}

fn is_hex_sha(value: &str) -> bool {
    (7..=64).contains(&value.len()) && value.chars().all(|character| character.is_ascii_hexdigit())
}

fn path_is_allowed(path: &str, allowed_paths: &[String]) -> bool {
    let path = path.replace('\\', "/");
    if path.starts_with('/') || path.contains(':') || path.split('/').any(|part| part == "..") {
        return false;
    }
    allowed_paths.iter().any(|allowed| {
        let allowed = allowed.trim().replace('\\', "/");
        let allowed = allowed
            .strip_prefix("./")
            .unwrap_or(&allowed)
            .trim_matches('/');
        if allowed == "." {
            return true;
        }
        !allowed.is_empty() && (path == allowed || path.starts_with(&format!("{allowed}/")))
    })
}

#[derive(Debug, Error)]
pub enum GitError {
    #[error("path is not a Git working tree: {0}")]
    NotRepository(PathBuf),
    #[error("system git is unavailable")]
    GitUnavailable,
    #[error("Git operation failed: {0}")]
    Io(String),
    #[error("invalid Git remote name: {0}")]
    InvalidRemoteName(String),
    #[error("invalid Git remote URL: {0}")]
    InvalidRemoteUrl(String),
    #[error("configured remote URL does not match: expected {expected}, found {actual}")]
    RemoteMismatch { expected: String, actual: String },
    #[error("working tree is not clean; changed paths: {0:?}")]
    DirtyWorkingTree(Vec<String>),
    #[error("work branch already exists: {0}")]
    BranchExists(String),
    #[error("work branch does not exist: {0}")]
    BranchNotFound(String),
    #[error("branch is protected: {0}")]
    ProtectedBranch(String),
    #[error("invalid branch name: {0}")]
    InvalidBranchName(String),
    #[error("invalid base ref: {0}")]
    InvalidBaseRef(String),
    #[error("base ref does not exist: {0}")]
    BaseRefNotFound(String),
    #[error("commit does not exist: {0}")]
    CommitNotFound(String),
    #[error("commit {actual} is not the head of branch {branch}; expected {expected}")]
    CommitNotBranchHead {
        branch: String,
        expected: String,
        actual: String,
    },
    #[error("changes are outside the task's allowed paths: {0:?}")]
    ChangesOutsideAllowedPaths(Vec<String>),
    #[error("git {operation} failed: {stderr}")]
    CommandFailed {
        operation: &'static str,
        stderr: String,
    },
}

impl GitError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotRepository(_) => "git_not_repository",
            Self::GitUnavailable | Self::Io(_) | Self::CommandFailed { .. } => "git_failed",
            Self::InvalidRemoteName(_)
            | Self::InvalidRemoteUrl(_)
            | Self::RemoteMismatch { .. } => "git_remote_invalid",
            Self::DirtyWorkingTree(_) => "git_worktree_dirty",
            Self::BranchExists(_) => "git_branch_exists",
            Self::BranchNotFound(_) | Self::BaseRefNotFound(_) => "git_ref_not_found",
            Self::ProtectedBranch(_) => "git_branch_protected",
            Self::InvalidBranchName(_) | Self::InvalidBaseRef(_) => "git_ref_invalid",
            Self::CommitNotFound(_) => "git_commit_not_found",
            Self::CommitNotBranchHead { .. } => "git_commit_not_branch_head",
            Self::ChangesOutsideAllowedPaths(_) => "git_changes_outside_allowed_paths",
        }
    }
}
