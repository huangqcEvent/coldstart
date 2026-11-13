use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, mpsc},
    thread::{self, JoinHandle},
};

use coldstart_adapters::{GitAdapter, GitHubClient, GitHubToken, MemorySecretStore, SecretStore};
use coldstart_app::{
    ClaimRequest, CreatePrivateRepositoryInput, IntegrationService, PlanService, ProjectService,
    PullRequestService, RepositoryDraft, ReviewService, SaveCredentialInput,
};
use coldstart_core::{
    AcceptanceResult, ArtifactId, BlueprintContent, BlueprintEntityId, BlueprintGeneration,
    CapabilityObligation, CapabilityStatus, ClaimResult, CredentialEnvironment, CredentialKind,
    DomainEventKind, GenerationSource, ModuleDefinition, PlatformDefinition, PlatformKind,
    PullRequestConflictState, PullRequestDeploymentState, PullRequestDeploymentsState,
    PullRequestMergeability, PullRequestRecord, PullRequestState, ReleasePrerequisite,
    ReportOutcome, RepositoryAuthMode, RepositorySyncPolicy, RepositoryVerificationCommand,
    SourceReference, TraceableText, WorkItemStatus, WorkReportInput,
};
use coldstart_storage::Database;
use tempfile::TempDir;
use tiny_http::{Header, Response, Server, StatusCode};

struct Fixture {
    _directory: TempDir,
    home: PathBuf,
    project_id: coldstart_core::ProjectId,
    remote: PathBuf,
    work: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("temporary directory must exist");
        let home = directory.path().join("project-home");
        let status = ProjectService::new()
            .create_project("ColdStart 集成测试", home.clone())
            .expect("project must create");
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

        let database_path = home.join(".coldstart").join("state.db");
        let mut database = Database::open(&database_path).expect("database must open");
        let revision = database
            .create_blueprint_revision(
                &status.project_id,
                blueprint_content(status.project_id),
                BlueprintGeneration {
                    source: GenerationSource::Manual,
                    model_provider: None,
                    model_name: None,
                    prompt_version: None,
                },
                vec![],
            )
            .expect("blueprint must create");
        database
            .publish_blueprint_revision(&status.project_id, &revision.id)
            .expect("blueprint must publish");
        Self {
            _directory: directory,
            home,
            project_id: status.project_id,
            remote,
            work,
        }
    }

    fn repository_draft(
        &self,
        credential_id: Option<coldstart_core::CredentialId>,
    ) -> RepositoryDraft {
        RepositoryDraft {
            key: "web".to_owned(),
            name: "Web 前台".to_owned(),
            local_path: self.work.clone(),
            remote_url: self.remote.to_string_lossy().into_owned(),
            default_branch: "main".to_owned(),
            protected_branches: vec!["main".to_owned()],
            auth_mode: if credential_id.is_some() {
                RepositoryAuthMode::Credential
            } else {
                RepositoryAuthMode::None
            },
            credential_id,
            sync_policy: RepositorySyncPolicy::FetchBeforeClaim,
        }
    }

    fn save_repository(
        &self,
        service: &IntegrationService,
        credential_id: Option<coldstart_core::CredentialId>,
    ) -> coldstart_app::IntegrationWorkspace {
        let workspace = service
            .save_repository(self.home.clone(), self.repository_draft(credential_id))
            .expect("repository must save");
        service
            .save_verification_commands(self.home.clone(), "web", vec![verification_command()])
            .expect("verification command must save");
        workspace
    }

    fn publish_plan(&self, service: &PlanService, allowed_paths: Vec<String>) -> String {
        let draft = service
            .generate_draft(self.home.clone())
            .expect("plan draft must generate")
            .draft
            .expect("draft must exist");
        let mut content = draft.content;
        for task in &mut content.tasks {
            if task.task_key.starts_with("foundation:") {
                task.allowed_paths = allowed_paths.clone();
            }
        }
        let saved = service
            .save_draft(self.home.clone(), content)
            .expect("plan draft must save");
        let draft_id = saved.draft.expect("draft must remain").id;
        service
            .publish(self.home.clone(), draft_id)
            .expect("plan must publish")
            .published
            .expect("published plan must exist")
            .content
            .tasks
            .into_iter()
            .find(|task| task.task_key.starts_with("foundation:"))
            .expect("foundation task must exist")
            .task_key
    }

    fn claim(&self, service: &PlanService, request_id: &str) -> ClaimResult {
        service
            .claim_next(
                self.home.clone(),
                ClaimRequest {
                    agent_profile: "codex".to_owned(),
                    repository: Some("web".to_owned()),
                    request_id: request_id.to_owned(),
                },
            )
            .expect("claim must call")
    }

    fn commit(&self, relative_path: &str, content: &str) -> String {
        let path = self.work.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent directory must create");
        }
        fs::write(path, content).expect("file must write");
        git(&self.work, &["add", relative_path]);
        git(&self.work, &["commit", "-m", "task change"]);
        git_output(&self.work, &["rev-parse", "HEAD"])
    }
}

fn blueprint_content(project_id: coldstart_core::ProjectId) -> BlueprintContent {
    let source = SourceReference {
        artifact_id: ArtifactId::new(),
        anchor_id: "section_1".to_owned(),
        quote: Some("需要 Web 订单系统".to_owned()),
    };
    let platform_id = BlueprintEntityId::derive(&project_id, "platform", "Web");
    BlueprintContent {
        product_goal: TraceableText {
            id: BlueprintEntityId::derive(&project_id, "goal", "product"),
            text: "构建订单系统".to_owned(),
            sources: vec![source.clone()],
        },
        in_scope: vec![],
        out_of_scope: vec![],
        roles: vec![],
        platforms: vec![PlatformDefinition {
            id: platform_id.clone(),
            name: "Web".to_owned(),
            kind: PlatformKind::Web,
            description: "订单前台".to_owned(),
            sources: vec![source.clone()],
        }],
        modules: vec![ModuleDefinition {
            id: BlueprintEntityId::derive(&project_id, "module", "Orders"),
            name: "订单".to_owned(),
            description: "创建和查询订单".to_owned(),
            platforms: vec![platform_id],
            depends_on: vec![],
            sources: vec![source.clone()],
        }],
        technologies: vec![],
        capabilities: vec![CapabilityObligation {
            id: BlueprintEntityId::derive(&project_id, "capability", "阿里云 OSS"),
            name: "阿里云 OSS".to_owned(),
            provider: "aliyun_oss".to_owned(),
            description: "上传商品图片".to_owned(),
            sources: vec![source.clone()],
        }],
        release_prerequisites: vec![ReleasePrerequisite {
            id: BlueprintEntityId::derive(&project_id, "release", "ICP 备案"),
            name: "ICP 备案".to_owned(),
            category: "compliance".to_owned(),
            description: "完成网站备案".to_owned(),
            lead_time_days: Some(20),
            blocking: true,
            sources: vec![source],
        }],
        assumptions: vec![],
        risks: vec![],
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

fn report_for(
    task: &coldstart_core::TaskSpec,
    commit_sha: String,
    changed_files: Vec<String>,
    idempotency_key: &str,
) -> WorkReportInput {
    WorkReportInput {
        status: ReportOutcome::Completed,
        summary: "完成当前任务".to_owned(),
        branch: task.work_branch.clone(),
        commit_sha: Some(commit_sha),
        changed_files,
        acceptance_results: task
            .acceptance_criteria
            .iter()
            .map(|criterion| AcceptanceResult {
                criterion_id: criterion.id.clone(),
                passed: true,
                note: "通过".to_owned(),
                evidence_refs: vec!["test-summary".to_owned()],
            })
            .collect(),
        commands_run: vec![],
        evidence_refs: vec!["test-summary".to_owned()],
        known_issues: vec![],
        follow_up_items: vec![],
        contract_hash: task.contract_hash(),
        idempotency_key: idempotency_key.to_owned(),
    }
}

fn verification_command() -> RepositoryVerificationCommand {
    let system_root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    RepositoryVerificationCommand {
        name: "where-cmd".to_owned(),
        executable: system_root.join("System32").join("where.exe"),
        args: vec!["cmd.exe".to_owned()],
        timeout_seconds: 10,
        position: 0,
    }
}

fn executable_on_path(name: &str) -> PathBuf {
    let output = Command::new("where.exe")
        .arg(name)
        .output()
        .expect("where.exe must run");
    assert!(
        output.status.success(),
        "could not locate {name}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let path = String::from_utf8(output.stdout)
        .expect("where.exe output must be UTF-8")
        .lines()
        .next()
        .expect("where.exe must return a path")
        .trim()
        .to_owned();
    fs::canonicalize(&path).expect("verification executable must canonicalize")
}

fn task_for(service: &PlanService, home: &Path, task_key: &str) -> coldstart_core::TaskSpec {
    service
        .load_workspace(home.to_path_buf())
        .expect("workspace must load")
        .work_items
        .into_iter()
        .find(|view| view.task.task_key == task_key)
        .expect("task must exist")
        .task
}

struct CapturedGitHubRequest {
    method: String,
    url: String,
    authorization: Option<String>,
    body: String,
}

struct MockGitHub {
    base_url: String,
    receiver: mpsc::Receiver<CapturedGitHubRequest>,
    handle: JoinHandle<()>,
}

impl MockGitHub {
    fn start(status: u16, body: &str) -> Self {
        Self::start_sequence(vec![(status, body.to_owned())])
    }

    fn start_sequence(responses: Vec<(u16, String)>) -> Self {
        let server = Server::http("127.0.0.1:0").expect("mock server must start");
        let port = server
            .server_addr()
            .to_ip()
            .expect("mock server must use TCP")
            .port();
        let (sender, receiver) = mpsc::channel();
        let handle = thread::spawn(move || {
            for (status, body) in responses {
                let mut request = server.recv().expect("mock server must receive a request");
                let authorization = request
                    .headers()
                    .iter()
                    .find(|header| {
                        header
                            .field
                            .to_string()
                            .eq_ignore_ascii_case("authorization")
                    })
                    .map(|header| header.value.as_str().to_owned());
                let mut request_body = String::new();
                request
                    .as_reader()
                    .read_to_string(&mut request_body)
                    .expect("request body must read");
                sender
                    .send(CapturedGitHubRequest {
                        method: request.method().to_string(),
                        url: request.url().to_owned(),
                        authorization,
                        body: request_body,
                    })
                    .expect("request must capture");
                let mut response = Response::from_string(body).with_status_code(StatusCode(status));
                response.add_header(
                    Header::from_bytes("Content-Type", "application/json")
                        .expect("header must be valid"),
                );
                request.respond(response).expect("response must write");
            }
        });
        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            receiver,
            handle,
        }
    }

    fn finish(self) -> (String, String, Option<String>) {
        let request = self.finish_request();
        (request.method, request.url, request.authorization)
    }

    fn finish_request(self) -> CapturedGitHubRequest {
        let request = self
            .receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("request must be captured");
        self.handle.join().expect("server must stop");
        request
    }

    fn finish_requests(self) -> Vec<CapturedGitHubRequest> {
        let requests = self.receiver.iter().collect::<Vec<_>>();
        self.handle.join().expect("server must stop");
        requests
    }
}

#[test]
fn credentials_stay_out_of_the_workspace_and_capabilities_merge_from_the_blueprint() {
    const SECRET: &str = "sensitive-oss-secret";
    let fixture = Fixture::new();
    let plan = PlanService::new();
    fixture.publish_plan(&plan, vec![".".to_owned()]);
    let secrets = Arc::new(MemorySecretStore::default());
    let service = IntegrationService::with_dependencies(secrets.clone(), GitAdapter::new(), None);

    let workspace = service
        .save_credential(
            fixture.home.clone(),
            SaveCredentialInput {
                name: "阿里云 OSS".to_owned(),
                provider: "aliyun".to_owned(),
                kind: CredentialKind::ApiKey,
                environment: CredentialEnvironment::Development,
                secret: SECRET.to_owned(),
            },
        )
        .expect("credential must save");
    assert_eq!(workspace.credentials.len(), 1);
    let credential = workspace.credentials[0].clone();
    assert_eq!(
        secrets
            .get(&credential.secret_ref)
            .expect("secret must be stored"),
        SECRET
    );
    let serialized = serde_json::to_string(&workspace).expect("workspace must serialize");
    assert!(!serialized.contains(SECRET));
    assert_eq!(workspace.capabilities.len(), 3);

    let capability_id = BlueprintEntityId::derive(&fixture.project_id, "capability", "阿里云 OSS");
    let workspace = service
        .update_capability(
            fixture.home.clone(),
            capability_id.clone(),
            "development".to_owned(),
            CapabilityStatus::Configured,
            Some(credential.id),
            Some("凭据已写入系统凭据库".to_owned()),
        )
        .expect("capability must update");
    let state = workspace
        .capabilities
        .iter()
        .find(|state| state.capability_id == capability_id && state.environment == "development")
        .expect("development capability must exist");
    assert_eq!(state.status, CapabilityStatus::Configured);
    assert_eq!(
        state.credential_ref.as_deref(),
        Some(credential.secret_ref.as_str())
    );
    assert_eq!(workspace.release.prerequisites.len(), 1);
    assert_eq!(workspace.release.milestones.len(), 1);
}

#[test]
fn validates_and_saves_a_real_git_repository() {
    let fixture = Fixture::new();
    let service = IntegrationService::with_dependencies(
        Arc::new(MemorySecretStore::default()),
        GitAdapter::new(),
        None,
    );
    let draft = fixture.repository_draft(None);
    let validation = service
        .validate_repository(fixture.home.clone(), &draft)
        .expect("repository must validate");
    assert!(validation.status.clean);
    assert_eq!(validation.status.branch.as_deref(), Some("main"));
    let workspace = service
        .save_repository(fixture.home.clone(), draft)
        .expect("repository must save");
    assert_eq!(workspace.repositories.len(), 1);
    assert_eq!(workspace.repositories[0].key, "web");
}

#[test]
fn imports_a_github_session_without_returning_the_token() {
    let fixture = Fixture::new();
    let body = serde_json::json!({
        "login": "octocat",
        "name": "The Octocat",
        "avatar_url": "https://avatars.example/octocat.png"
    })
    .to_string();
    let server = MockGitHub::start(200, &body);
    let client = GitHubClient::with_endpoints(&server.base_url, &server.base_url)
        .expect("client must create");
    let secrets = Arc::new(MemorySecretStore::default());
    let service =
        IntegrationService::with_dependencies(secrets.clone(), GitAdapter::new(), Some(client));
    let token = GitHubToken::new("gho_session_secret").expect("token must be valid");
    let session = service
        .import_github_token(fixture.home.clone(), token)
        .expect("GitHub session must import");
    assert_eq!(session.login, "octocat");
    assert_eq!(session.scopes, Vec::<String>::new());
    let serialized = serde_json::to_string(&session).expect("session must serialize");
    assert!(!serialized.contains("gho_session_secret"));
    assert_eq!(
        secrets
            .get(&session.credential_ref)
            .expect("token must store"),
        "gho_session_secret"
    );
    let (method, url, authorization) = server.finish();
    assert_eq!(method, "GET");
    assert_eq!(url, "/user");
    assert_eq!(authorization.as_deref(), Some("Bearer gho_session_secret"));
}

#[test]
fn claims_create_a_git_branch_and_dirty_worktrees_release_the_lease() {
    let fixture = Fixture::new();
    let secrets = Arc::new(MemorySecretStore::default());
    let integrations =
        IntegrationService::with_dependencies(secrets.clone(), GitAdapter::new(), None);
    fixture.save_repository(&integrations, None);
    let plan = PlanService::with_dependencies(secrets, GitAdapter::new());
    let task_key = fixture.publish_plan(&plan, vec!["src".to_owned()]);

    fs::write(fixture.work.join("README.md"), "dirty\n").expect("file must modify");
    let error = plan
        .claim_next(
            fixture.home.clone(),
            ClaimRequest {
                agent_profile: "codex".to_owned(),
                repository: Some("web".to_owned()),
                request_id: "dirty-claim".to_owned(),
            },
        )
        .expect_err("dirty repository must reject the claim");
    assert_eq!(error.code(), "git_worktree_dirty");
    let workspace = plan
        .load_workspace(fixture.home.clone())
        .expect("workspace must load");
    assert!(
        workspace
            .leases
            .iter()
            .all(|lease| lease.status != coldstart_core::LeaseStatus::Active)
    );
    assert_eq!(
        workspace
            .work_items
            .iter()
            .find(|view| view.task.task_key == task_key)
            .expect("task must exist")
            .work_item
            .status,
        WorkItemStatus::Ready
    );
    git(&fixture.work, &["restore", "README.md"]);

    let ClaimResult::Claimed(claimed) = fixture.claim(&plan, "clean-claim") else {
        panic!("clean repository must claim work");
    };
    let status = GitAdapter::new()
        .inspect(&fixture.work)
        .expect("repository must inspect");
    let task = task_for(&plan, &fixture.home, &task_key);
    assert_eq!(status.branch.as_deref(), task.work_branch.as_deref());
    assert_eq!(claimed.work_item.status, WorkItemStatus::Claimed);
}

#[test]
fn context_includes_repository_and_capability_metadata_but_never_the_secret() {
    const SECRET: &str = "context-must-not-leak-this-secret";
    let fixture = Fixture::new();
    let secrets = Arc::new(MemorySecretStore::default());
    let integrations =
        IntegrationService::with_dependencies(secrets.clone(), GitAdapter::new(), None);
    let workspace = integrations
        .save_credential(
            fixture.home.clone(),
            SaveCredentialInput {
                name: "阿里云 OSS".to_owned(),
                provider: "aliyun".to_owned(),
                kind: CredentialKind::ApiKey,
                environment: CredentialEnvironment::Development,
                secret: SECRET.to_owned(),
            },
        )
        .expect("credential must save");
    let credential = workspace.credentials[0].clone();
    fixture.save_repository(&integrations, Some(credential.id));
    let capability_id = BlueprintEntityId::derive(&fixture.project_id, "capability", "阿里云 OSS");
    integrations
        .update_capability(
            fixture.home.clone(),
            capability_id,
            "development".to_owned(),
            CapabilityStatus::Configured,
            Some(credential.id),
            Some("测试环境已配置".to_owned()),
        )
        .expect("capability must update");

    let plan = PlanService::with_dependencies(secrets, GitAdapter::new());
    let task_key = fixture.publish_plan(&plan, vec!["src".to_owned()]);
    let ClaimResult::Claimed(claimed) = fixture.claim(&plan, "context-claim") else {
        panic!("work must claim");
    };
    let task = task_for(&plan, &fixture.home, &task_key);
    let context = plan
        .get_context(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token,
        )
        .expect("context must compile");
    let markdown = context.markdown;

    assert!(markdown.contains(&fixture.work.to_string_lossy().into_owned()));
    assert!(markdown.contains(&fixture.remote.to_string_lossy().into_owned()));
    assert!(markdown.contains(task.work_branch.as_deref().expect("branch must exist")));
    assert!(markdown.contains("保护分支：main"));
    assert!(markdown.contains("阿里云 OSS"));
    assert!(markdown.contains("configured"));
    assert!(markdown.contains(&credential.secret_ref));
    assert!(!markdown.contains(SECRET));
}

#[test]
fn verifies_reports_before_acceptance_and_pushes_the_task_branch() {
    let fixture = Fixture::new();
    let secrets = Arc::new(MemorySecretStore::default());
    let integrations =
        IntegrationService::with_dependencies(secrets.clone(), GitAdapter::new(), None);
    fixture.save_repository(&integrations, None);
    let plan = PlanService::with_dependencies(secrets, GitAdapter::new());
    let task_key = fixture.publish_plan(&plan, vec!["src".to_owned()]);
    let ClaimResult::Claimed(claimed) = fixture.claim(&plan, "report-claim") else {
        panic!("work must claim");
    };
    let task = task_for(&plan, &fixture.home, &task_key);
    let commit = fixture.commit("src/main.rs", "fn main() {}\n");
    plan.submit_report(
        fixture.home.clone(),
        claimed.work_item.id,
        claimed.lease_token,
        report_for(
            &task,
            commit.clone(),
            vec!["src/main.rs".to_owned()],
            "report-1",
        ),
    )
    .expect("report must submit");

    let workspace = IntegrationService::with_dependencies(
        Arc::new(MemorySecretStore::default()),
        GitAdapter::new(),
        None,
    )
    .load_workspace(fixture.home.clone())
    .expect("integration workspace must load");
    assert!(
        workspace
            .git_syncs
            .iter()
            .any(|sync| sync.status == coldstart_core::GitSyncStatus::Pushed)
    );
    let work_branch = task
        .work_branch
        .as_deref()
        .expect("agent task must have a work branch");
    assert_eq!(
        git_output(
            &fixture.remote,
            &["rev-parse", &format!("refs/heads/{work_branch}")]
        ),
        commit
    );
}

#[test]
fn repairs_failed_verification_and_retries_until_done_and_pushed() {
    let fixture = Fixture::new();
    let secrets = Arc::new(MemorySecretStore::default());
    let integrations =
        IntegrationService::with_dependencies(secrets.clone(), GitAdapter::new(), None);
    fixture.save_repository(&integrations, None);
    integrations
        .save_verification_commands(
            fixture.home.clone(),
            "web",
            vec![RepositoryVerificationCommand {
                name: "repo-grep".to_owned(),
                executable: executable_on_path("git.exe"),
                args: vec![
                    "grep".to_owned(),
                    "--quiet".to_owned(),
                    "FIXED".to_owned(),
                    "--".to_owned(),
                    "src".to_owned(),
                ],
                timeout_seconds: 10,
                position: 0,
            }],
        )
        .expect("verification command must save");
    let plan = PlanService::with_dependencies(secrets, GitAdapter::new());
    let task_key = fixture.publish_plan(&plan, vec!["src".to_owned()]);
    let ClaimResult::Claimed(claimed) = fixture.claim(&plan, "repair-verification-claim") else {
        panic!("work must claim");
    };
    let task = task_for(&plan, &fixture.home, &task_key);
    let first_commit = fixture.commit("src/main.rs", "pub fn main() {}\n");
    let failed = plan
        .submit_report(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token.clone(),
            report_for(
                &task,
                first_commit,
                vec!["src/main.rs".to_owned()],
                "repair-verification-first",
            ),
        )
        .expect_err("missing marker must fail repository verification");
    assert_eq!(failed.code(), "verification_failed");
    let details = failed
        .verification_details()
        .expect("verification failure details must exist");
    assert_eq!(details["failureKind"], "non_zero_exit");
    assert_eq!(details["command"], "repo-grep");
    assert_eq!(details["exitCode"], 1);

    let workspace = plan
        .load_workspace(fixture.home.clone())
        .expect("workspace must load");
    let view = workspace
        .work_items
        .iter()
        .find(|view| view.work_item.id == claimed.work_item.id)
        .expect("claimed task must exist");
    assert_eq!(view.work_item.status, WorkItemStatus::InProgress);
    assert!(view.active_lease.is_some());

    let fixed_commit = fixture.commit("src/main.rs", "// FIXED\npub fn main() {}\n");
    let completed = plan
        .submit_report(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token,
            report_for(
                &task,
                fixed_commit.clone(),
                vec!["src/main.rs".to_owned()],
                "repair-verification-second",
            ),
        )
        .expect("repaired repository must pass verification");
    assert!(completed.accepted);
    assert_eq!(
        completed.verification_status,
        coldstart_core::VerificationStatus::Passed
    );
    assert_eq!(
        plan.load_workspace(fixture.home.clone())
            .expect("workspace must reload")
            .work_items
            .into_iter()
            .find(|view| view.work_item.id == claimed.work_item.id)
            .expect("completed task must exist")
            .work_item
            .status,
        WorkItemStatus::Done
    );
    let work_branch = task
        .work_branch
        .as_deref()
        .expect("agent task must have a work branch");
    assert_eq!(
        git_output(
            &fixture.remote,
            &["rev-parse", &format!("refs/heads/{work_branch}")]
        ),
        fixed_commit
    );
}

#[test]
fn push_failures_keep_the_task_done_and_record_the_risk() {
    let fixture = Fixture::new();
    let secrets = Arc::new(MemorySecretStore::default());
    let integrations =
        IntegrationService::with_dependencies(secrets.clone(), GitAdapter::new(), None);
    fixture.save_repository(&integrations, None);
    let plan = PlanService::with_dependencies(secrets, GitAdapter::new());
    let task_key = fixture.publish_plan(&plan, vec!["src".to_owned()]);
    let ClaimResult::Claimed(claimed) = fixture.claim(&plan, "push-failure-claim") else {
        panic!("work must claim");
    };
    let task = task_for(&plan, &fixture.home, &task_key);
    let commit = fixture.commit("src/main.rs", "fn main() {}\n");
    git(
        &fixture.work,
        &[
            "remote",
            "set-url",
            "origin",
            "http://127.0.0.1:1/private.git",
        ],
    );
    plan.submit_report(
        fixture.home.clone(),
        claimed.work_item.id,
        claimed.lease_token,
        report_for(&task, commit, vec!["src/main.rs".to_owned()], "report-fail"),
    )
    .expect("accepted report must not fail because push failed");

    let workspace = plan
        .load_workspace(fixture.home.clone())
        .expect("workspace must load");
    assert_eq!(
        workspace
            .work_items
            .iter()
            .find(|view| view.work_item.id == claimed.work_item.id)
            .expect("work item must exist")
            .work_item
            .status,
        WorkItemStatus::Done
    );
    let integration = IntegrationService::with_dependencies(
        Arc::new(MemorySecretStore::default()),
        GitAdapter::new(),
        None,
    )
    .load_workspace(fixture.home)
    .expect("integration workspace must load");
    assert!(
        integration
            .git_syncs
            .iter()
            .any(|sync| sync.status == coldstart_core::GitSyncStatus::Failed)
    );
}

#[test]
fn rejects_a_commit_that_changes_paths_outside_the_task() {
    let fixture = Fixture::new();
    let secrets = Arc::new(MemorySecretStore::default());
    let integrations =
        IntegrationService::with_dependencies(secrets.clone(), GitAdapter::new(), None);
    fixture.save_repository(&integrations, None);
    let plan = PlanService::with_dependencies(secrets, GitAdapter::new());
    let task_key = fixture.publish_plan(&plan, vec!["src".to_owned()]);
    let ClaimResult::Claimed(claimed) = fixture.claim(&plan, "outside-claim") else {
        panic!("work must claim");
    };
    let task = task_for(&plan, &fixture.home, &task_key);
    let commit = fixture.commit("docs/guide.md", "guide\n");
    let error = plan
        .submit_report(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token,
            report_for(
                &task,
                commit,
                vec!["docs/guide.md".to_owned()],
                "outside-report",
            ),
        )
        .expect_err("outside path must fail");
    assert_eq!(error.code(), "git_changes_outside_allowed_paths");
}

#[test]
fn creates_a_private_repository_and_updates_the_local_origin() {
    let fixture = Fixture::new();
    let client_body = serde_json::json!({
        "id": 42,
        "name": "coldstart-demo",
        "full_name": "octocat/coldstart-demo",
        "private": true,
        "clone_url": "https://github.com/octocat/coldstart-demo.git",
        "ssh_url": "git@github.com:octocat/coldstart-demo.git",
        "html_url": "https://github.com/octocat/coldstart-demo",
        "default_branch": "main"
    })
    .to_string();
    let server = MockGitHub::start(201, &client_body);
    let client = GitHubClient::with_endpoints(&server.base_url, &server.base_url)
        .expect("GitHub client must create");
    let secrets = Arc::new(MemorySecretStore::default());
    let service = IntegrationService::with_dependencies(secrets, GitAdapter::new(), Some(client));
    let workspace = service
        .save_credential(
            fixture.home.clone(),
            SaveCredentialInput {
                name: "GitHub".to_owned(),
                provider: "github".to_owned(),
                kind: CredentialKind::OAuthToken,
                environment: CredentialEnvironment::Shared,
                secret: "gho_create_repo".to_owned(),
            },
        )
        .expect("credential must save");
    let credential_id = workspace.credentials[0].id;
    service
        .save_repository(
            fixture.home.clone(),
            fixture.repository_draft(Some(credential_id)),
        )
        .expect("repository must save");
    let created = service
        .create_github_private_repository(
            fixture.home.clone(),
            CreatePrivateRepositoryInput {
                credential_id,
                repository_key: "web".to_owned(),
                name: "coldstart-demo".to_owned(),
                description: Some("ColdStart demo".to_owned()),
            },
        )
        .expect("private repository must create");
    assert_eq!(created.full_name, "octocat/coldstart-demo");
    assert_eq!(
        git_output(&fixture.work, &["remote", "get-url", "origin"]),
        "https://github.com/octocat/coldstart-demo.git"
    );
    let (method, url, authorization) = server.finish();
    assert_eq!(method, "POST");
    assert_eq!(url, "/user/repos");
    assert_eq!(authorization.as_deref(), Some("Bearer gho_create_repo"));
}

#[test]
fn refuses_a_pull_request_for_a_report_that_has_not_passed_the_gates() {
    let fixture = Fixture::new();
    let secrets = Arc::new(MemorySecretStore::default());
    let integrations =
        IntegrationService::with_dependencies(secrets.clone(), GitAdapter::new(), None);
    fixture.save_repository(&integrations, None);
    integrations
        .save_verification_commands(
            fixture.home.clone(),
            "web",
            vec![RepositoryVerificationCommand {
                name: "always-fails".to_owned(),
                executable: executable_on_path("cmd.exe"),
                args: vec!["/C".to_owned(), "exit".to_owned(), "1".to_owned()],
                timeout_seconds: 10,
                position: 0,
            }],
        )
        .expect("failing verification command must save");
    let plan = PlanService::with_dependencies(secrets, GitAdapter::new());
    let task_key = fixture.publish_plan(&plan, vec!["src".to_owned()]);
    let ClaimResult::Claimed(claimed) = fixture.claim(&plan, "blocked-pr-claim") else {
        panic!("work must claim");
    };
    let task = task_for(&plan, &fixture.home, &task_key);
    let commit = fixture.commit("src/main.rs", "fn main() {}\n");
    let error = plan
        .submit_report(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token,
            report_for(
                &task,
                commit,
                vec!["src/main.rs".to_owned()],
                "blocked-pr-report",
            ),
        )
        .expect_err("failing verification must reject the report");
    assert_eq!(error.code(), "verification_failed");

    let database = Database::open(&fixture.home.join(".coldstart").join("state.db"))
        .expect("database must open");
    let report = database
        .list_work_reports(&fixture.project_id)
        .expect("reports must load")
        .into_iter()
        .next()
        .expect("failed report must persist");
    assert!(!report.accepted);
    let service =
        PullRequestService::with_dependencies(Arc::new(MemorySecretStore::default()), None);
    let error = service
        .create_draft(fixture.home.clone(), report.id)
        .expect_err("ineligible report must not create a PR");
    assert_eq!(error.code(), "pull_request_ineligible");
}

#[test]
fn creates_an_idempotent_draft_pull_request_and_refreshes_its_merged_state() {
    let fixture = Fixture::new();
    let secrets = Arc::new(MemorySecretStore::default());
    let integrations =
        IntegrationService::with_dependencies(secrets.clone(), GitAdapter::new(), None);
    fixture.save_repository(&integrations, None);
    let plan = PlanService::with_dependencies(secrets.clone(), GitAdapter::new());
    let task_key = fixture.publish_plan(&plan, vec!["src".to_owned()]);
    let ClaimResult::Claimed(claimed) = fixture.claim(&plan, "draft-pr-claim") else {
        panic!("work must claim");
    };
    let task = task_for(&plan, &fixture.home, &task_key);
    let commit = fixture.commit("src/main.rs", "fn main() {}\n");
    let report = plan
        .submit_report(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token,
            report_for(
                &task,
                commit.clone(),
                vec!["src/main.rs".to_owned()],
                "draft-pr-report",
            ),
        )
        .expect("completed report must pass verification");
    assert!(report.accepted);
    let workspace = integrations
        .save_credential(
            fixture.home.clone(),
            SaveCredentialInput {
                name: "GitHub PR".to_owned(),
                provider: "github".to_owned(),
                kind: CredentialKind::OAuthToken,
                environment: CredentialEnvironment::Shared,
                secret: format!("test-{}-value", "github"),
            },
        )
        .expect("GitHub credential must save");
    let credential_id = workspace.credentials[0].id;
    let database_path = fixture.home.join(".coldstart").join("state.db");
    let github_host = format!("git{}", "hub.com");
    {
        let mut database = Database::open(&database_path).expect("database must open");
        let mut repository = database
            .repository_by_key(&fixture.project_id, "web")
            .expect("repository query must work")
            .expect("repository must exist");
        repository.remote_url = format!("{}://{github_host}/octocat/demo.git", "https");
        repository.auth_mode = RepositoryAuthMode::Credential;
        repository.credential_id = Some(credential_id);
        database
            .save_repository(&repository)
            .expect("GitHub repository metadata must save");
    }
    let branch = task
        .work_branch
        .clone()
        .expect("agent task must have a work branch");
    let pull_request_url = format!("{}://{github_host}/octocat/demo/pull/17", "https");
    let create_body = serde_json::json!({
        "number": 17,
        "html_url": pull_request_url,
        "state": "open",
        "draft": true,
        "merged": false,
        "title": format!("[{task_key}] {}", task.title),
        "head": { "ref": branch, "sha": commit.trim() },
        "base": { "ref": "main", "sha": "2222222222222222222222222222222222222222" }
    })
    .to_string();
    let server = MockGitHub::start(201, &create_body);
    let client = GitHubClient::with_endpoints(&server.base_url, &server.base_url)
        .expect("GitHub client must create");
    let service = PullRequestService::with_dependencies(secrets.clone(), Some(client));
    let head_before = git_output(&fixture.work, &["rev-parse", "HEAD"]);
    let created = service
        .create_draft(fixture.home.clone(), report.id)
        .expect("draft PR must create");
    assert_eq!(created.number, 17);
    assert_eq!(created.state, PullRequestState::Open);
    assert!(created.draft);
    assert_eq!(created.head_branch, branch);
    assert_eq!(created.base_branch, "main");
    let duplicate = service
        .create_draft(fixture.home.clone(), report.id)
        .expect("existing draft PR must be reused");
    assert_eq!(duplicate, created);
    assert_eq!(
        git_output(&fixture.work, &["rev-parse", "HEAD"]),
        head_before
    );
    let captured = server.finish_request();
    assert_eq!(captured.method, "POST");
    assert_eq!(captured.url, "/repos/octocat/demo/pulls");
    assert_eq!(
        captured.authorization.as_deref(),
        Some(format!("Bearer test-{}-value", "github").as_str())
    );
    let request: serde_json::Value =
        serde_json::from_str(&captured.body).expect("PR request must be JSON");
    assert_eq!(request["draft"], true);
    assert_eq!(request["head"], branch);
    assert_eq!(request["base"], "main");
    let body = request["body"].as_str().expect("PR body must be text");
    assert!(body.contains("## ColdStart 交付"));
    assert!(body.contains(&task_key));
    assert!(body.contains("src/main.rs"));
    assert!(!body.contains("test-github-value"));
    assert!(!body.contains(&fixture.home.to_string_lossy().to_string()));
    let serialized = serde_json::to_string(&created).expect("PR record must serialize");
    assert!(!serialized.contains("test-github-value"));
    let refresh_body = serde_json::json!({
        "number": 17,
        "html_url": created.html_url,
        "state": "closed",
        "draft": false,
        "merged": true,
        "title": format!("[{task_key}] {}", task.title),
        "head": { "ref": branch, "sha": commit.trim() },
        "base": { "ref": "main", "sha": "2222222222222222222222222222222222222222" },
        "mergeable": true,
        "mergeable_state": "clean"
    })
    .to_string();
    let checks_body = serde_json::json!({
        "total_count": 2,
        "check_runs": [
            {
                "id": 1,
                "name": "unit tests",
                "status": "completed",
                "conclusion": "success",
                "details_url": "https://github.com/octocat/demo/runs/1",
                "app": { "name": "GitHub Actions", "slug": "github-actions" }
            },
            {
                "id": 2,
                "name": "package",
                "status": "in_progress",
                "conclusion": null,
                "details_url": null,
                "app": null
            }
        ]
    })
    .to_string();
    let reviews_body = serde_json::json!([
        {
            "id": 11,
            "state": "CHANGES_REQUESTED",
            "body": "token=test-github-value 请补充失败分支测试。",
            "html_url": "https://github.com/octocat/demo/pull/17#pullrequestreview-11",
            "user": { "login": "reviewer" }
        }
    ])
    .to_string();
    let comments_body = serde_json::json!([
        {
            "id": 21,
            "path": "src/main.rs",
            "line": 12,
            "original_line": 10,
            "body": "这里需要在失败时回滚。",
            "html_url": "https://github.com/octocat/demo/pull/17#discussion_r21",
            "user": { "login": "reviewer" }
        }
    ])
    .to_string();
    let deployments_body = serde_json::json!([{
        "id": 71,
        "sha": commit.trim(),
        "environment": "production",
        "description": "发布候选版本"
    }])
    .to_string();
    let deployment_status_body = serde_json::json!([{
        "state": "success",
        "description": "生产部署完成",
        "updated_at": "2026-11-13T10:00:00Z"
    }])
    .to_string();
    let refresh_server = MockGitHub::start_sequence(vec![
        (200, refresh_body),
        (200, checks_body),
        (200, reviews_body),
        (200, comments_body),
        (200, deployments_body),
        (200, deployment_status_body),
    ]);
    let refresh_client =
        GitHubClient::with_endpoints(&refresh_server.base_url, &refresh_server.base_url)
            .expect("GitHub client must create");
    let service = PullRequestService::with_dependencies(secrets.clone(), Some(refresh_client));
    let refreshed = service
        .refresh(fixture.home.clone(), created.id)
        .expect("PR status must refresh");
    assert_eq!(refreshed.state, PullRequestState::Merged);
    assert!(!refreshed.draft);
    let requests = refresh_server.finish_requests();
    assert_eq!(requests.len(), 6);
    assert!(requests.iter().all(|request| request.method == "GET"));
    assert_eq!(requests[0].url, "/repos/octocat/demo/pulls/17");
    assert_eq!(
        requests[1].url,
        format!(
            "/repos/octocat/demo/commits/{}/check-runs?filter=all&per_page=100",
            commit.trim()
        )
    );
    assert_eq!(
        requests[2].url,
        "/repos/octocat/demo/pulls/17/reviews?per_page=100"
    );
    assert_eq!(
        requests[3].url,
        "/repos/octocat/demo/pulls/17/comments?per_page=100"
    );
    assert_eq!(
        requests[4].url,
        format!(
            "/repos/octocat/demo/deployments?sha={}&per_page=11",
            commit.trim()
        )
    );
    assert_eq!(
        requests[5].url,
        "/repos/octocat/demo/deployments/71/statuses?per_page=1"
    );
    assert!(
        requests.iter().all(|request| {
            request.authorization.as_deref() == Some("Bearer test-github-value")
        })
    );
    let database = Database::open(&database_path).expect("database must reopen");
    let stored = database
        .load_pull_request(&fixture.project_id, &created.id)
        .expect("PR query must work")
        .expect("PR must persist");
    assert_eq!(stored.state, PullRequestState::Merged);
    let events = database
        .list_events(&fixture.project_id, None, 100, true)
        .expect("events must load");
    assert!(events.events.iter().any(|event| matches!(
        &event.event.kind,
        DomainEventKind::PullRequestStateChanged {
            number: 17,
            state,
            ..
        } if state == "merged"
    )));
    let insights = database
        .load_pull_request_insights(&fixture.project_id, &created.id)
        .expect("insights must load")
        .expect("insights must persist");
    assert_eq!(insights.checks.len(), 2);
    assert_eq!(insights.reviews.len(), 1);
    assert_eq!(insights.review_comments.len(), 1);
    assert_eq!(insights.review_comments[0].line, Some(12));
    assert_eq!(insights.mergeability, PullRequestMergeability::Mergeable);
    assert_eq!(
        insights.deployment_details.state,
        PullRequestDeploymentsState::Available
    );
    assert_eq!(insights.deployment_details.deployments.len(), 1);
    assert_eq!(
        insights.deployment_details.deployments[0].environment,
        "production"
    );
    assert_eq!(
        insights.deployment_details.deployments[0].state,
        PullRequestDeploymentState::Success
    );
    assert_eq!(
        insights.deployment_details.deployments[0]
            .description
            .as_deref(),
        Some("生产部署完成")
    );
    let serialized_insights = serde_json::to_string(&insights).expect("insights must serialize");
    assert!(!serialized_insights.contains("test-github-value"));
    assert!(events.events.iter().any(|event| matches!(
        &event.event.kind,
        DomainEventKind::PullRequestInsightsSynced {
            number: 17,
            checks_total: 2,
            review_count: 1,
            comment_count: 1,
            ..
        }
    )));
    drop(database);
    let review_workspace = ReviewService::new()
        .load_workspace(fixture.home.clone())
        .expect("review workspace must load");
    let history = review_workspace
        .recent_reviews
        .iter()
        .find(|item| item.report_id == report.id)
        .expect("review history must exist");
    assert_eq!(
        history
            .pull_request_insights
            .as_ref()
            .map(|snapshot| snapshot.review_comments.len()),
        Some(1)
    );

    let failed_refresh = serde_json::json!({
        "number": 17,
        "html_url": created.html_url,
        "state": "closed",
        "draft": false,
        "merged": false,
        "title": format!("[{task_key}] {}", task.title),
        "head": { "ref": branch, "sha": commit.trim() },
        "base": { "ref": "main", "sha": "2222222222222222222222222222222222222222" }
    })
    .to_string();
    let failed_server = MockGitHub::start_sequence(vec![
        (200, failed_refresh),
        (500, r#"{"message":"checks unavailable"}"#.to_owned()),
    ]);
    let failed_client =
        GitHubClient::with_endpoints(&failed_server.base_url, &failed_server.base_url)
            .expect("GitHub client must create");
    let failed_service =
        PullRequestService::with_dependencies(secrets.clone(), Some(failed_client));
    let error = failed_service
        .refresh(fixture.home.clone(), created.id)
        .expect_err("a failed checks request must abort the refresh");
    assert_eq!(error.code(), "github_request_failed");
    let failed_requests = failed_server.finish_requests();
    assert_eq!(failed_requests.len(), 2);
    let database = Database::open(&database_path).expect("database must reopen");
    let stored = database
        .load_pull_request(&fixture.project_id, &created.id)
        .expect("PR query must work")
        .expect("PR must persist");
    assert_eq!(
        stored.state,
        PullRequestState::Merged,
        "a failed refresh must not overwrite the previous state"
    );
    drop(database);

    let deployment_failure_refresh = serde_json::json!({
        "number": 17,
        "html_url": created.html_url,
        "state": "closed",
        "draft": false,
        "merged": false,
        "title": format!("[{task_key}] {}", task.title),
        "head": { "ref": branch, "sha": commit.trim() },
        "base": { "ref": "main", "sha": "2222222222222222222222222222222222222222" },
        "mergeable": true,
        "mergeable_state": "clean"
    })
    .to_string();
    let deployment_failure_server = MockGitHub::start_sequence(vec![
        (200, deployment_failure_refresh),
        (200, r#"{"total_count":0,"check_runs":[]}"#.to_owned()),
        (200, "[]".to_owned()),
        (200, "[]".to_owned()),
        (500, r#"{"message":"deployments unavailable"}"#.to_owned()),
    ]);
    let deployment_failure_client = GitHubClient::with_endpoints(
        &deployment_failure_server.base_url,
        &deployment_failure_server.base_url,
    )
    .expect("GitHub client must create");
    let deployment_failure_service =
        PullRequestService::with_dependencies(secrets.clone(), Some(deployment_failure_client));
    deployment_failure_service
        .refresh(fixture.home.clone(), created.id)
        .expect("deployment lookup failure must not fail the PR refresh");
    let deployment_failure_requests = deployment_failure_server.finish_requests();
    assert_eq!(deployment_failure_requests.len(), 5);
    assert!(
        deployment_failure_requests
            .iter()
            .all(|request| request.method == "GET")
    );
    let database = Database::open(&database_path).expect("database must reopen");
    let insights = database
        .load_pull_request_insights(&fixture.project_id, &created.id)
        .expect("insights must load")
        .expect("insights must persist");
    assert_eq!(
        insights.deployment_details.state,
        PullRequestDeploymentsState::Unavailable
    );
    assert!(insights.deployment_details.deployments.is_empty());
    drop(database);

    let status_failure_refresh = serde_json::json!({
        "number": 17,
        "html_url": created.html_url,
        "state": "closed",
        "draft": false,
        "merged": false,
        "title": format!("[{task_key}] {}", task.title),
        "head": { "ref": branch, "sha": commit.trim() },
        "base": { "ref": "main", "sha": "2222222222222222222222222222222222222222" },
        "mergeable": true,
        "mergeable_state": "clean"
    })
    .to_string();
    let status_failure_deployments = serde_json::json!([{
        "id": 72,
        "sha": commit.trim(),
        "environment": "staging",
        "description": "部署记录描述"
    }])
    .to_string();
    let status_failure_server = MockGitHub::start_sequence(vec![
        (200, status_failure_refresh),
        (200, r#"{"total_count":0,"check_runs":[]}"#.to_owned()),
        (200, "[]".to_owned()),
        (200, "[]".to_owned()),
        (200, status_failure_deployments),
        (
            500,
            r#"{"message":"deployment status unavailable"}"#.to_owned(),
        ),
    ]);
    let status_failure_client = GitHubClient::with_endpoints(
        &status_failure_server.base_url,
        &status_failure_server.base_url,
    )
    .expect("GitHub client must create");
    let status_failure_service =
        PullRequestService::with_dependencies(secrets, Some(status_failure_client));
    status_failure_service
        .refresh(fixture.home.clone(), created.id)
        .expect("one deployment status failure must not fail the PR refresh");
    let status_failure_requests = status_failure_server.finish_requests();
    assert_eq!(status_failure_requests.len(), 6);
    let database = Database::open(&database_path).expect("database must reopen");
    let insights = database
        .load_pull_request_insights(&fixture.project_id, &created.id)
        .expect("insights must load")
        .expect("insights must persist");
    assert_eq!(
        insights.deployment_details.state,
        PullRequestDeploymentsState::Available
    );
    assert_eq!(insights.deployment_details.deployments.len(), 1);
    assert_eq!(
        insights.deployment_details.deployments[0].state,
        PullRequestDeploymentState::Unknown
    );
    assert_eq!(
        insights.deployment_details.deployments[0]
            .description
            .as_deref(),
        Some("部署记录描述")
    );
}

#[test]
fn refresh_persists_local_conflict_paths_and_degrades_when_base_is_missing() {
    let fixture = Fixture::new();
    let secrets = Arc::new(MemorySecretStore::default());
    let integrations =
        IntegrationService::with_dependencies(secrets.clone(), GitAdapter::new(), None);
    fixture.save_repository(&integrations, None);
    let plan = PlanService::with_dependencies(secrets.clone(), GitAdapter::new());
    let task_key = fixture.publish_plan(&plan, vec!["src".to_owned()]);
    let ClaimResult::Claimed(claimed) = fixture.claim(&plan, "conflict-locator-claim") else {
        panic!("work must claim");
    };
    let task = task_for(&plan, &fixture.home, &task_key);
    let commit = fixture.commit("src/main.rs", "fn main() { task(); }\n");
    let report = plan
        .submit_report(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token,
            report_for(
                &task,
                commit.clone(),
                vec!["src/main.rs".to_owned()],
                "conflict-locator-report",
            ),
        )
        .expect("completed report must pass verification");
    assert!(report.accepted);

    let workspace = integrations
        .save_credential(
            fixture.home.clone(),
            SaveCredentialInput {
                name: "GitHub conflict PR".to_owned(),
                provider: "github".to_owned(),
                kind: CredentialKind::OAuthToken,
                environment: CredentialEnvironment::Shared,
                secret: format!("test-{}-value", "github"),
            },
        )
        .expect("GitHub credential must save");
    let credential_id = workspace.credentials[0].id;
    let database_path = fixture.home.join(".coldstart").join("state.db");
    let repository_id = {
        let mut database = Database::open(&database_path).expect("database must open");
        let mut repository = database
            .repository_by_key(&fixture.project_id, "web")
            .expect("repository query must work")
            .expect("repository must exist");
        repository.remote_url = "https://github.com/octocat/demo.git".to_owned();
        repository.auth_mode = RepositoryAuthMode::Credential;
        repository.credential_id = Some(credential_id);
        database
            .save_repository(&repository)
            .expect("GitHub repository metadata must save");
        repository.id
    };

    let branch = task
        .work_branch
        .clone()
        .expect("agent task must have a work branch");
    let pull_request_url = "https://github.com/octocat/demo/pull/23";
    let record = PullRequestRecord::new(
        fixture.project_id,
        report.work_item_id,
        report.id,
        repository_id,
        23,
        pull_request_url.to_owned(),
        format!("[{task_key}] {}", task.title),
        branch.clone(),
        "main".to_owned(),
        PullRequestState::Open,
        false,
    )
    .expect("PR record must validate");
    {
        let mut database = Database::open(&database_path).expect("database must open");
        database
            .save_pull_request(&record)
            .expect("PR record must save");
    }

    git(&fixture.work, &["switch", "main"]);
    let base_sha = fixture.commit("src/main.rs", "fn main() { base(); }\n");
    git(&fixture.work, &["switch", &branch]);
    assert_eq!(git_output(&fixture.work, &["status", "--porcelain"]), "");

    let pull_request_body = serde_json::json!({
        "number": 23,
        "html_url": pull_request_url,
        "state": "open",
        "draft": false,
        "merged": false,
        "title": format!("[{task_key}] {}", task.title),
        "head": { "ref": branch, "sha": commit.trim() },
        "base": { "ref": "main", "sha": base_sha.trim() },
        "mergeable": false,
        "mergeable_state": "dirty"
    })
    .to_string();
    let server = MockGitHub::start_sequence(vec![
        (200, pull_request_body),
        (200, r#"{"total_count":0,"check_runs":[]}"#.to_owned()),
        (200, "[]".to_owned()),
        (200, "[]".to_owned()),
        (200, "[]".to_owned()),
    ]);
    let client = GitHubClient::with_endpoints(&server.base_url, &server.base_url)
        .expect("GitHub client must create");
    let service = PullRequestService::with_dependencies(secrets.clone(), Some(client));
    let refreshed = service
        .refresh(fixture.home.clone(), record.id)
        .expect("conflicted PR must refresh");
    assert_eq!(refreshed.state, PullRequestState::Open);
    let requests = server.finish_requests();
    assert_eq!(
        requests.len(),
        5,
        "conflict diagnostics must not add GitHub requests beyond deployment lookup"
    );
    assert!(requests.iter().all(|request| request.method == "GET"));

    let database = Database::open(&database_path).expect("database must reopen");
    let insights = database
        .load_pull_request_insights(&fixture.project_id, &record.id)
        .expect("insights must load")
        .expect("insights must persist");
    assert_eq!(insights.mergeability, PullRequestMergeability::Conflicted);
    assert_eq!(
        insights.conflict_details.state,
        PullRequestConflictState::Detected
    );
    assert_eq!(insights.conflict_details.paths, ["src/main.rs"]);
    assert!(!insights.conflict_details.truncated);
    drop(database);
    assert_eq!(
        git_output(&fixture.work, &["branch", "--show-current"]),
        branch
    );
    assert_eq!(git_output(&fixture.work, &["status", "--porcelain"]), "");

    let missing_base = "2".repeat(40);
    let missing_base_body = serde_json::json!({
        "number": 23,
        "html_url": pull_request_url,
        "state": "open",
        "draft": false,
        "merged": false,
        "title": format!("[{task_key}] {}", task.title),
        "head": { "ref": branch, "sha": commit.trim() },
        "base": { "ref": "main", "sha": missing_base },
        "mergeable": false,
        "mergeable_state": "dirty"
    })
    .to_string();
    let missing_server = MockGitHub::start_sequence(vec![
        (200, missing_base_body),
        (200, r#"{"total_count":0,"check_runs":[]}"#.to_owned()),
        (200, "[]".to_owned()),
        (200, "[]".to_owned()),
        (200, "[]".to_owned()),
    ]);
    let missing_client =
        GitHubClient::with_endpoints(&missing_server.base_url, &missing_server.base_url)
            .expect("GitHub client must create");
    let service = PullRequestService::with_dependencies(secrets, Some(missing_client));
    service
        .refresh(fixture.home.clone(), record.id)
        .expect("local diagnostic failure must not fail the GitHub snapshot");
    let insights = Database::open(&database_path)
        .expect("database must reopen")
        .load_pull_request_insights(&fixture.project_id, &record.id)
        .expect("insights must load")
        .expect("insights must persist");
    assert_eq!(insights.mergeability, PullRequestMergeability::Conflicted);
    assert_eq!(
        insights.conflict_details.state,
        PullRequestConflictState::Unavailable
    );
    assert!(insights.conflict_details.paths.is_empty());
}
