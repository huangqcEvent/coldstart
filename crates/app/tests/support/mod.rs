use std::{
    collections::VecDeque,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use coldstart_adapters::{
    GitAuth, GitCommitVerification, GitError, MemorySecretStore, VerificationCommandExecution,
    VerificationCommandExecutor,
};
use coldstart_app::{PlanGitGateway, PlanService};
use coldstart_core::{
    ProjectId, Repository, RepositoryAuthMode, RepositorySyncPolicy, RepositoryVerificationCommand,
};
use coldstart_storage::Database;

#[derive(Default)]
pub struct FakePlanGit {
    changed_files: Mutex<Vec<String>>,
    checkout_errors: Mutex<VecDeque<String>>,
    push_error: Mutex<Option<String>>,
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
}

impl PlanGitGateway for FakePlanGit {
    fn fetch(&self, _repository: &Path, _auth: &GitAuth) -> Result<(), GitError> {
        Ok(())
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
