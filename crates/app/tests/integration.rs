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
    RepositoryDraft, SaveCredentialInput,
};
use coldstart_core::{
    AcceptanceResult, ArtifactId, BlueprintContent, BlueprintEntityId, BlueprintGeneration,
    CapabilityObligation, CapabilityStatus, ClaimResult, CredentialEnvironment, CredentialKind,
    GenerationSource, ModuleDefinition, PlatformDefinition, PlatformKind, ReleasePrerequisite,
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

struct MockGitHub {
    base_url: String,
    receiver: mpsc::Receiver<(String, String, Option<String>)>,
    handle: JoinHandle<()>,
}

impl MockGitHub {
    fn start(status: u16, body: &str) -> Self {
        let server = Server::http("127.0.0.1:0").expect("mock server must start");
        let port = server
            .server_addr()
            .to_ip()
            .expect("mock server must use TCP")
            .port();
        let (sender, receiver) = mpsc::channel();
        let body = body.to_owned();
        let handle = thread::spawn(move || {
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
                .send((
                    request.method().to_string(),
                    request.url().to_owned(),
                    authorization,
                ))
                .expect("request must capture");
            let mut response = Response::from_string(body).with_status_code(StatusCode(status));
            response.add_header(
                Header::from_bytes("Content-Type", "application/json")
                    .expect("header must be valid"),
            );
            request.respond(response).expect("response must write");
        });
        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            receiver,
            handle,
        }
    }

    fn finish(self) -> (String, String, Option<String>) {
        let request = self
            .receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("request must be captured");
        self.handle.join().expect("server must stop");
        request
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
