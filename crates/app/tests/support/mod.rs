#![allow(dead_code)]

use std::{
    collections::VecDeque,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use coldstart_adapters::{
    GitAuth, GitCommitVerification, GitError, MemorySecretStore, ReviewDiff, ReviewDiffFile,
    ReviewModel, ReviewModelError, ReviewModelOutput, ReviewModelRequest,
    VerificationCommandExecution, VerificationCommandExecutor,
};
use coldstart_app::{PlanGitGateway, PlanService};
use coldstart_core::{
    ArtifactFormat, ArtifactId, ArtifactMetadata, DocumentAnchor, ProjectId, Repository,
    RepositoryAuthMode, RepositorySyncPolicy, RepositoryVerificationCommand,
};
use coldstart_storage::Database;

/// 写入一份最小但真实的来源制品，供蓝图来源引用通过存储层校验。
pub fn seed_requirement_artifact(database: &mut Database, project_id: ProjectId) -> ArtifactId {
    let artifact_id = ArtifactId::new();
    database
        .insert_artifact(
            &ArtifactMetadata {
                id: artifact_id,
                project_id,
                file_name: "requirements.md".to_owned(),
                format: ArtifactFormat::Markdown,
                content_hash: format!("{artifact_id}-content"),
                text_hash: format!("{artifact_id}-text"),
                byte_size: 128,
                import_source: "requirements.md".to_owned(),
                imported_at: chrono::Utc::now(),
            },
            &[DocumentAnchor {
                id: "section_1".to_owned(),
                label: "需求".to_owned(),
                text: "订单系统需求".to_owned(),
            }],
        )
        .expect("requirement artifact must insert");
    artifact_id
}

#[derive(Default)]
pub struct FakePlanGit {
    changed_files: Mutex<Vec<String>>,
    checkout_errors: Mutex<VecDeque<String>>,
    push_error: Mutex<Option<String>>,
    review_diff: Mutex<Option<ReviewDiff>>,
    push_calls: AtomicUsize,
    verify_calls: AtomicUsize,
}

impl FakePlanGit {
    pub fn passing() -> Self {
        Self {
            changed_files: Mutex::new(vec!["src/main.rs".to_owned()]),
            ..Self::default()
        }
    }

    pub fn fail_checkout(&self, messages: Vec<String>) {
        *self.checkout_errors.lock().expect("checkout errors lock") =
            messages.into_iter().collect();
    }

    pub fn fail_push(&self, message: impl Into<String>) {
        *self.push_error.lock().expect("push error lock") = Some(message.into());
    }

    pub fn push_calls(&self) -> usize {
        self.push_calls.load(Ordering::SeqCst)
    }

    pub fn verify_calls(&self) -> usize {
        self.verify_calls.load(Ordering::SeqCst)
    }

    pub fn set_review_diff(&self, diff: ReviewDiff) {
        *self.review_diff.lock().expect("review diff lock") = Some(diff);
    }
}

impl PlanGitGateway for FakePlanGit {
    fn fetch(&self, _repository: &Path, _auth: &GitAuth) -> Result<(), GitError> {
        Ok(())
    }

    fn resolve_commit(&self, _repository: &Path, _reference: &str) -> Result<String, GitError> {
        Ok("a".repeat(40))
    }

    fn validate_remote(
        &self,
        _repository: &Path,
        _remote_name: &str,
        _expected_url: &str,
    ) -> Result<(), GitError> {
        Ok(())
    }

    fn create_work_branch(
        &self,
        _repository: &Path,
        _base_ref: &str,
        _branch: &str,
        _protected_branches: &[String],
        _auth: &GitAuth,
    ) -> Result<(), GitError> {
        Ok(())
    }

    fn verify_report_commit(
        &self,
        _repository: &Path,
        branch: &str,
        _base_ref: &str,
        commit_sha: &str,
        _allowed_paths: &[String],
        _protected_branches: &[String],
    ) -> Result<GitCommitVerification, GitError> {
        self.verify_calls.fetch_add(1, Ordering::SeqCst);
        Ok(GitCommitVerification {
            commit_sha: commit_sha.to_owned(),
            branch: branch.to_owned(),
            changed_files: self
                .changed_files
                .lock()
                .expect("changed files lock")
                .clone(),
        })
    }

    fn ensure_verification_checkout(
        &self,
        _repository: &Path,
        _branch: &str,
        _commit_sha: &str,
    ) -> Result<(), GitError> {
        let error = self
            .checkout_errors
            .lock()
            .expect("checkout errors lock")
            .pop_front();
        match error {
            Some(message) => Err(GitError::VerificationTrackedChanges(vec![message])),
            None => Ok(()),
        }
    }

    fn extract_review_diff(
        &self,
        _repository: &Path,
        base_commit: &str,
        head_commit: &str,
    ) -> Result<ReviewDiff, GitError> {
        if let Some(diff) = self.review_diff.lock().expect("review diff lock").clone() {
            return Ok(diff);
        }
        let changed_files = self
            .changed_files
            .lock()
            .expect("changed files lock")
            .clone();
        let patch = changed_files
            .iter()
            .map(|path| format!("diff --git a/{path} b/{path}\n+review fixture\n"))
            .collect::<String>();
        Ok(ReviewDiff {
            base_commit: base_commit.to_owned(),
            head_commit: head_commit.to_owned(),
            files: changed_files
                .into_iter()
                .map(|path| ReviewDiffFile {
                    path,
                    status: "M".to_owned(),
                    added: 1,
                    deleted: 0,
                    binary: false,
                })
                .collect(),
            binary_files: Vec::new(),
            character_count: patch.chars().count(),
            patch,
        })
    }

    fn push_branch(
        &self,
        _repository: &Path,
        _branch: &str,
        _protected_branches: &[String],
        _auth: &GitAuth,
    ) -> Result<(), GitError> {
        self.push_calls.fetch_add(1, Ordering::SeqCst);
        match self.push_error.lock().expect("push error lock").clone() {
            Some(message) => Err(GitError::CommandFailed {
                operation: "push",
                stderr: message,
            }),
            None => Ok(()),
        }
    }
}

#[derive(Default)]
pub struct ScriptedExecutor {
    executions: Mutex<VecDeque<VerificationCommandExecution>>,
    calls: Mutex<Vec<String>>,
}

impl ScriptedExecutor {
    pub fn new(executions: Vec<VerificationCommandExecution>) -> Self {
        Self {
            executions: Mutex::new(executions.into()),
            calls: Mutex::new(Vec::new()),
        }
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("executor calls lock").clone()
    }

    pub fn failure(
        exit_code: i32,
        output_summary: impl Into<String>,
    ) -> VerificationCommandExecution {
        VerificationCommandExecution {
            exit_code: Some(exit_code),
            duration_ms: 12,
            output_summary: output_summary.into(),
            truncated: false,
            timed_out: false,
            launch_error: None,
            infrastructure_error: None,
        }
    }

    pub fn timeout() -> VerificationCommandExecution {
        VerificationCommandExecution {
            exit_code: Some(1),
            duration_ms: 1_000,
            output_summary: "verification timed out".to_owned(),
            truncated: false,
            timed_out: true,
            launch_error: None,
            infrastructure_error: None,
        }
    }

    pub fn launch_failure() -> VerificationCommandExecution {
        VerificationCommandExecution {
            exit_code: None,
            duration_ms: 3,
            output_summary: String::new(),
            truncated: false,
            timed_out: false,
            launch_error: Some("could not start verification executable".to_owned()),
            infrastructure_error: None,
        }
    }
}

impl VerificationCommandExecutor for ScriptedExecutor {
    fn execute(
        &self,
        command: &RepositoryVerificationCommand,
        _repository: &Path,
    ) -> VerificationCommandExecution {
        self.calls
            .lock()
            .expect("executor calls lock")
            .push(command.name.clone());
        self.executions
            .lock()
            .expect("scripted executions lock")
            .pop_front()
            .unwrap_or(VerificationCommandExecution {
                exit_code: Some(0),
                duration_ms: 5,
                output_summary: "ok".to_owned(),
                truncated: false,
                timed_out: false,
                launch_error: None,
                infrastructure_error: None,
            })
    }
}

pub fn service(git: Arc<FakePlanGit>, executor: Arc<ScriptedExecutor>) -> PlanService {
    PlanService::with_gateways(Arc::new(MemorySecretStore::default()), git, executor)
}

pub fn service_with_secrets(
    secrets: Arc<MemorySecretStore>,
    git: Arc<FakePlanGit>,
    executor: Arc<ScriptedExecutor>,
) -> PlanService {
    PlanService::with_gateways(secrets, git, executor)
}

pub struct ScriptedReviewModel {
    responses: Mutex<VecDeque<Result<ReviewModelOutput, ReviewModelError>>>,
    calls: AtomicUsize,
}

impl ScriptedReviewModel {
    pub fn new(responses: Vec<Result<ReviewModelOutput, ReviewModelError>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            calls: AtomicUsize::new(0),
        }
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl ReviewModel for ScriptedReviewModel {
    fn review(&self, _request: &ReviewModelRequest) -> Result<ReviewModelOutput, ReviewModelError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.responses
            .lock()
            .expect("review responses lock")
            .pop_front()
            .unwrap_or(Err(ReviewModelError::Request(
                "no scripted review response".to_owned(),
            )))
    }
}

pub fn service_with_review_model(
    secrets: Arc<MemorySecretStore>,
    git: Arc<FakePlanGit>,
    executor: Arc<ScriptedExecutor>,
    review_model: Arc<ScriptedReviewModel>,
) -> PlanService {
    PlanService::with_review_model(secrets, git, executor, review_model)
}

pub fn review_output(findings: Vec<coldstart_core::ReviewFinding>) -> ReviewModelOutput {
    ReviewModelOutput {
        summary: "Review 完成".to_owned(),
        findings,
    }
}

pub fn configure_repository(
    home: &Path,
    project_id: ProjectId,
    repository_key: &str,
    with_verification: bool,
) {
    let mut database =
        Database::open(&home.join(".coldstart").join("state.db")).expect("test database must open");
    let repository = Repository::new(
        project_id,
        repository_key.to_owned(),
        format!("{repository_key} 测试仓库"),
        home.join("repositories").join(repository_key),
        format!("https://example.invalid/{repository_key}.git"),
        "main".to_owned(),
        vec!["main".to_owned()],
        RepositoryAuthMode::None,
        None,
        RepositorySyncPolicy::FetchBeforeClaim,
    )
    .expect("test repository must be valid");
    database
        .save_repository(&repository)
        .expect("test repository must save");
    if with_verification {
        database
            .replace_repository_verification_commands(
                &project_id,
                repository_key,
                vec![verification_command("verify")],
            )
            .expect("test verification commands must save");
    }
}

pub fn verification_command(name: &str) -> RepositoryVerificationCommand {
    RepositoryVerificationCommand {
        name: name.to_owned(),
        executable: std::env::current_exe().expect("test executable path"),
        args: vec!["--list".to_owned()],
        timeout_seconds: 60,
        position: 0,
    }
}
