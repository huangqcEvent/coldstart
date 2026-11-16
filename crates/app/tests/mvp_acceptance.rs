use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use coldstart_adapters::{GitAdapter, MemorySecretStore, OpenAiCompatibleModel, SecretStore};
use coldstart_app::{
    ClaimRequest, IntegrationService, PlanService, ProjectService, RepositoryDraft,
    RequirementsService, SaveCredentialInput,
};
use coldstart_core::{
    AcceptanceCriterion, AcceptancePolicy, AcceptanceResult, ArtifactId, BlueprintContent,
    BlueprintEntityId, CapabilityObligation, CapabilityStatus, ClaimResult, CredentialEnvironment,
    CredentialKind, GitSyncStatus, ModuleDefinition, PlanRevisionId, PlatformDefinition,
    PlatformKind, ProjectId, ReleasePrerequisite, ReportOutcome, RepositoryAuthMode,
    RepositorySyncPolicy, RepositoryVerificationCommand, SourceReference, TaskKind, TaskSpec,
    TechnologyChoice, TraceableText, WorkItemStatus, WorkReportInput,
};
use coldstart_storage::Database;
use rusqlite::Connection;
use tempfile::{Builder, TempDir};

const OSS_SECRET: &str = "mvp-acceptance-oss-secret-must-not-persist";

struct Fixture {
    _directory: TempDir,
    home: PathBuf,
    root: PathBuf,
    project_id: ProjectId,
    artifact_id: ArtifactId,
    anchor_id: String,
    anchor_text: String,
}

impl Fixture {
    fn new() -> Self {
        let acceptance_root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("mvp-acceptance");
        fs::create_dir_all(&acceptance_root).expect("acceptance root must create");
        let directory = Builder::new()
            .prefix("coldstart-mvp-")
            .tempdir_in(&acceptance_root)
            .expect("temporary directory must exist on the build drive");
        let root = directory.path().to_path_buf();
        let home = root.join("project-home");
        let status = ProjectService::new()
            .create_project("ColdStart MVP 验收", home.clone())
            .expect("project must create");
        let source_path = root.join("requirements.md");
        fs::write(
            &source_path,
            "# 订单系统\n\n## 产品目标\n\n建设一个覆盖 Web、管理后台、Android 和微信小程序的订单系统。\n\n## 外部服务\n\n文件上传使用阿里云 OSS，密钥不得进入模型上下文。\n\n## 发布要求\n\n准备 Android 商店资料和微信小程序主体材料。\n",
        )
        .expect("requirements source must write");
        let requirements = RequirementsService::new(OpenAiCompatibleModel::new());
        let imported = requirements
            .import_document(home.clone(), source_path)
            .expect("requirements document must import");
        let artifact = imported
            .sources
            .first()
            .expect("artifact must exist")
            .clone();
        let anchors = imported
            .source_anchors
            .get(&artifact.id.to_string())
            .expect("artifact anchors must exist");
        let anchor = anchors.first().expect("anchor must exist").clone();
        Self {
            _directory: directory,
            home,
            root,
            project_id: status.project_id,
            artifact_id: artifact.id,
            anchor_id: anchor.id,
            anchor_text: anchor.text,
        }
    }

    fn source(&self) -> SourceReference {
        SourceReference {
            artifact_id: self.artifact_id,
            anchor_id: self.anchor_id.clone(),
            quote: Some(self.anchor_text.chars().take(200).collect()),
        }
    }
}

fn blueprint_content(project_id: ProjectId, source: SourceReference) -> BlueprintContent {
    let server_id = BlueprintEntityId::derive(&project_id, "platform", "Server");
    let web_id = BlueprintEntityId::derive(&project_id, "platform", "Web");
    let admin_id = BlueprintEntityId::derive(&project_id, "platform", "Admin");
    let app_id = BlueprintEntityId::derive(&project_id, "platform", "Android");
    let mini_program_id = BlueprintEntityId::derive(&project_id, "platform", "MiniProgram");
    let platforms = vec![
        PlatformDefinition {
            id: server_id.clone(),
            name: "服务端".to_owned(),
            kind: PlatformKind::Server,
            description: "Rust 订单服务".to_owned(),
            sources: vec![source.clone()],
        },
        PlatformDefinition {
            id: web_id.clone(),
            name: "Web 前台".to_owned(),
            kind: PlatformKind::Web,
            description: "面向顾客的 Web 下单前台".to_owned(),
            sources: vec![source.clone()],
        },
        PlatformDefinition {
            id: admin_id.clone(),
            name: "管理后台".to_owned(),
            kind: PlatformKind::Admin,
            description: "订单运营与售后管理".to_owned(),
            sources: vec![source.clone()],
        },
        PlatformDefinition {
            id: app_id.clone(),
            name: "Android App".to_owned(),
            kind: PlatformKind::Android,
            description: "Android 客户端".to_owned(),
            sources: vec![source.clone()],
        },
        PlatformDefinition {
            id: mini_program_id.clone(),
            name: "微信小程序".to_owned(),
            kind: PlatformKind::MiniProgram,
            description: "微信内下单入口".to_owned(),
            sources: vec![source.clone()],
        },
    ];
    let module_id = BlueprintEntityId::derive(&project_id, "module", "Orders");
    BlueprintContent {
        product_goal: TraceableText {
            id: BlueprintEntityId::derive(&project_id, "goal", "product"),
            text: "建设多端订单系统".to_owned(),
            sources: vec![source.clone()],
        },
        in_scope: vec![],
        out_of_scope: vec![],
        roles: vec![],
        platforms,
        modules: vec![ModuleDefinition {
            id: module_id,
            name: "订单".to_owned(),
            description: "创建、查询和处理订单".to_owned(),
            platforms: vec![server_id, web_id.clone(), admin_id, app_id, mini_program_id],
            depends_on: vec![],
            sources: vec![source.clone()],
        }],
        technologies: vec![TechnologyChoice {
            id: BlueprintEntityId::derive(&project_id, "technology", "Rust"),
            category: "backend".to_owned(),
            name: "Rust".to_owned(),
            reason: "服务端使用 Rust 实现".to_owned(),
            sources: vec![source.clone()],
        }],
        capabilities: vec![CapabilityObligation {
            id: BlueprintEntityId::derive(&project_id, "capability", "AliyunOSS"),
            name: "阿里云 OSS".to_owned(),
            provider: "aliyun_oss".to_owned(),
            description: "商品图片与订单附件上传".to_owned(),
            sources: vec![source.clone()],
        }],
        release_prerequisites: vec![
            ReleasePrerequisite {
                id: BlueprintEntityId::derive(&project_id, "release", "AndroidStore"),
                name: "Android 商店资料".to_owned(),
                category: "store".to_owned(),
                description: "准备签名主体、隐私政策和商店素材".to_owned(),
                lead_time_days: Some(10),
                blocking: true,
                sources: vec![source.clone()],
            },
            ReleasePrerequisite {
                id: BlueprintEntityId::derive(&project_id, "release", "MiniProgramReview"),
                name: "小程序主体与类目".to_owned(),
                category: "compliance".to_owned(),
                description: "确认小程序主体、服务类目和审核材料".to_owned(),
                lead_time_days: Some(5),
                blocking: true,
                sources: vec![source],
            },
        ],
        assumptions: vec![],
        risks: vec![],
    }
}

fn init_git_repository(remote: &Path, work: &Path) {
    git(
        remote.parent().expect("remote parent must exist"),
        &["init", "--bare", path(remote)],
    );
    fs::create_dir_all(work).expect("work tree must create");
    git(work, &["init", "-b", "main"]);
    git(work, &["config", "user.name", "ColdStart Acceptance"]);
    git(work, &["config", "user.email", "coldstart@example.invalid"]);
    fs::write(work.join("README.md"), "initial\n").expect("initial file must write");
    git(work, &["add", "README.md"]);
    git(work, &["commit", "-m", "initial"]);
    git(work, &["remote", "add", "origin", path(remote)]);
    git(work, &["push", "--set-upstream", "origin", "main"]);
}

fn commit_file(work: &Path, relative_path: &str, content: &str) -> String {
    let file = work.join(relative_path);
    fs::create_dir_all(file.parent().expect("file parent must exist"))
        .expect("file parent must create");
    fs::write(file, content).expect("task file must write");
    git(work, &["add", relative_path]);
    git(work, &["commit", "-m", "complete accepted task"]);
    git_output(work, &["rev-parse", "HEAD"])
}

fn report_for(task: &TaskSpec, commit_sha: String, changed_files: Vec<String>) -> WorkReportInput {
    WorkReportInput {
        status: ReportOutcome::Completed,
        summary: "完成工程基础并通过构建检查".to_owned(),
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
                evidence_refs: vec!["mvp-test-summary".to_owned()],
            })
            .collect(),
        commands_run: vec![],
        evidence_refs: vec!["mvp-test-summary".to_owned()],
        known_issues: vec![],
        follow_up_items: vec![],
        contract_hash: task.contract_hash(),
        idempotency_key: "mvp-acceptance-report".to_owned(),
    }
}

fn manual_review_task() -> TaskSpec {
    TaskSpec {
        task_key: "manual:launch-review".to_owned(),
        title: "人工复核上线资料".to_owned(),
        kind: TaskKind::HumanExternal,
        goal: "人工确认发布资料完整且可追溯".to_owned(),
        scope: vec!["发布资料".to_owned()],
        out_of_scope: vec!["不执行商店提审".to_owned()],
        module_id: None,
        platform_id: None,
        repository_key: None,
        base_ref: None,
        work_branch: None,
        allowed_paths: vec![],
        context_refs: vec![],
        acceptance_criteria: vec![AcceptanceCriterion {
            id: "readiness".to_owned(),
            text: "发布资料完整".to_owned(),
            required_evidence: true,
        }],
        acceptance_policy: AcceptancePolicy::HumanAcceptance,
        evidence_requirements: vec!["人工说明".to_owned()],
        priority: 10,
        critical_path: false,
        context_budget_tokens: 12_000,
        resource_keys: vec![],
        dependencies: vec![],
    }
}

fn view_status(workspace: &coldstart_app::PlanWorkspace, task_key: &str) -> WorkItemStatus {
    workspace
        .work_items
        .iter()
        .find(|view| view.task.task_key == task_key)
        .unwrap_or_else(|| panic!("task {task_key} must be in the active plan"))
        .work_item
        .status
}

fn active_plan_id(workspace: &coldstart_app::PlanWorkspace) -> PlanRevisionId {
    workspace
        .published
        .as_ref()
        .expect("published plan must exist")
        .id
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

fn path(path: &Path) -> &str {
    path.to_str().expect("acceptance path must be UTF-8")
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

#[test]
fn mvp_closes_requirement_to_release_and_survives_restart() {
    let fixture = Fixture::new();
    let requirements = RequirementsService::new(OpenAiCompatibleModel::new());
    let blueprint = blueprint_content(fixture.project_id, fixture.source());
    let saved_blueprint = requirements
        .save_blueprint(fixture.home.clone(), blueprint)
        .expect("manual blueprint must save");
    let blueprint_draft_id = saved_blueprint
        .draft
        .expect("blueprint draft must exist")
        .id;
    let published_blueprint = requirements
        .publish_blueprint(fixture.home.clone(), blueprint_draft_id)
        .expect("blueprint must publish");
    assert_eq!(
        published_blueprint
            .published
            .expect("published blueprint must exist")
            .id,
        blueprint_draft_id
    );

    let remote = fixture.root.join("remote.git");
    let work = fixture.root.join("server-work");
    init_git_repository(&remote, &work);
    let secrets = Arc::new(MemorySecretStore::default());
    let integrations =
        IntegrationService::with_dependencies(secrets.clone(), GitAdapter::new(), None);
    let integration_workspace = integrations
        .save_repository(
            fixture.home.clone(),
            RepositoryDraft {
                key: "server".to_owned(),
                name: "订单服务端".to_owned(),
                local_path: work.clone(),
                remote_url: path(&remote).to_owned(),
                default_branch: "main".to_owned(),
                protected_branches: vec!["main".to_owned()],
                auth_mode: RepositoryAuthMode::None,
                credential_id: None,
                sync_policy: RepositorySyncPolicy::FetchBeforeClaim,
            },
        )
        .expect("repository must save");
    assert_eq!(integration_workspace.repositories.len(), 1);
    integrations
        .save_verification_commands(fixture.home.clone(), "server", vec![verification_command()])
        .expect("verification command must save");
    let credential_workspace = integrations
        .save_credential(
            fixture.home.clone(),
            SaveCredentialInput {
                name: "阿里云 OSS".to_owned(),
                provider: "aliyun".to_owned(),
                kind: CredentialKind::ApiKey,
                environment: CredentialEnvironment::Development,
                secret: OSS_SECRET.to_owned(),
            },
        )
        .expect("credential must save");
    let credential = credential_workspace.credentials[0].clone();
    assert!(
        !serde_json::to_string(&credential_workspace)
            .expect("workspace must serialize")
            .contains(OSS_SECRET)
    );
    assert_eq!(
        secrets
            .get(&credential.secret_ref)
            .expect("secret must be available to the integration layer"),
        OSS_SECRET
    );
    let capability_id = BlueprintEntityId::derive(&fixture.project_id, "capability", "AliyunOSS");
    integrations
        .update_capability(
            fixture.home.clone(),
            capability_id.clone(),
            "development".to_owned(),
            CapabilityStatus::Configured,
            Some(credential.id),
            Some("系统凭据库已配置".to_owned()),
        )
        .expect("capability must update");

    let plan_service = PlanService::with_dependencies(secrets.clone(), GitAdapter::new());
    let generated = plan_service
        .generate_draft(fixture.home.clone())
        .expect("plan draft must generate");
    let first_draft = generated.draft.expect("plan draft must exist");
    let first_preview = plan_service
        .preview_publish(fixture.home.clone(), first_draft.id)
        .expect("initial preview must load");
    assert_eq!(first_preview.added.len(), first_draft.content.tasks.len());
    assert_eq!(first_preview.base_revision_id, None);
    assert!(first_preview.publishable);
    let repository_keys = first_draft
        .content
        .tasks
        .iter()
        .filter_map(|task| task.repository_key.as_deref())
        .collect::<std::collections::BTreeSet<_>>();
    for expected in ["server", "web", "admin", "app", "mini_program"] {
        assert!(
            repository_keys.contains(expected),
            "missing repository key {expected}"
        );
    }
    let first_workspace = plan_service
        .publish_confirmed(fixture.home.clone(), first_draft.id, false)
        .expect("initial plan must publish");
    let first_plan_id = active_plan_id(&first_workspace);

    let claimed = plan_service
        .claim_next(
            fixture.home.clone(),
            ClaimRequest {
                agent_profile: "codex".to_owned(),
                repository: Some("server".to_owned()),
                request_id: "mvp-claim-foundation".to_owned(),
            },
        )
        .expect("claim must call");
    let ClaimResult::Claimed(claimed) = claimed else {
        panic!("server foundation must be claimable");
    };
    let lease_token = claimed.lease_token.clone();
    let foundation_task = first_workspace
        .work_items
        .iter()
        .find(|view| view.work_item.id == claimed.work_item.id)
        .expect("claimed work must be visible")
        .task
        .clone();
    assert_eq!(foundation_task.repository_key.as_deref(), Some("server"));
    let foundation_key = foundation_task.task_key.clone();
    let work_branch = foundation_task
        .work_branch
        .clone()
        .expect("agent task must have a branch");
    assert_eq!(
        git_output(&work, &["branch", "--show-current"]),
        work_branch
    );
    let commit = commit_file(&work, "server/src/main.rs", "fn main() {}\n");
    let report = plan_service
        .submit_report(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token,
            report_for(
                &foundation_task,
                commit.clone(),
                vec!["server/src/main.rs".to_owned()],
            ),
        )
        .expect("completed report must submit");
    assert!(report.accepted);
    assert_eq!(
        git_output(
            &remote,
            &["rev-parse", &format!("refs/heads/{work_branch}")]
        ),
        commit
    );
    plan_service
        .mark_work_integrated(
            fixture.home.clone(),
            claimed.work_item.id,
            format!("test merge {commit}"),
        )
        .expect("foundation must be integrated before dependent work starts");
    let workspace_after_report = plan_service
        .load_workspace(fixture.home.clone())
        .expect("workspace must load");
    assert_eq!(
        view_status(&workspace_after_report, &foundation_key),
        WorkItemStatus::Done
    );
    assert!(workspace_after_report.work_items.iter().any(|view| {
        view.task.task_key.starts_with("contracts:")
            && view.work_item.status == WorkItemStatus::Ready
    }));

    let release = integrations
        .load_workspace(fixture.home.clone())
        .expect("release center must load");
    assert!(!release.release.prerequisites.is_empty());
    assert!(!release.release.milestones.is_empty());
    assert!(!release.release.human_tasks.is_empty());
    for view in release
        .release
        .prerequisites
        .iter()
        .chain(release.release.milestones.iter())
        .chain(release.release.approval_gates.iter())
        .chain(release.release.human_tasks.iter())
    {
        assert_eq!(view.work_item.plan_revision_id, first_plan_id);
    }
    assert!(release.capabilities.iter().any(|capability| {
        capability.capability_id == capability_id
            && capability.environment == "development"
            && capability.status == CapabilityStatus::Configured
    }));
    assert!(
        !serde_json::to_string(&release)
            .expect("release center must serialize")
            .contains(OSS_SECRET)
    );

    let contract_claim = plan_service
        .claim_next(
            fixture.home.clone(),
            ClaimRequest {
                agent_profile: "codex".to_owned(),
                repository: Some("server".to_owned()),
                request_id: "mvp-block-replan".to_owned(),
            },
        )
        .expect("second claim must call");
    let ClaimResult::Claimed(contract_claim) = contract_claim else {
        panic!("contracts must be claimable after foundation");
    };
    let blocked_candidate = plan_service
        .generate_draft(fixture.home.clone())
        .expect("replacement draft must generate")
        .draft
        .expect("replacement draft must exist");
    let blocked_preview = plan_service
        .preview_publish(fixture.home.clone(), blocked_candidate.id)
        .expect("blocked preview must load");
    assert_eq!(blocked_preview.active_lease_count, 1);
    assert!(!blocked_preview.publishable);
    let error = plan_service
        .publish_confirmed(fixture.home.clone(), blocked_candidate.id, true)
        .expect_err("an active lease must block replanning");
    assert_eq!(error.code(), "plan_lease_active");
    plan_service
        .release(
            fixture.home.clone(),
            contract_claim.work_item.id,
            contract_claim.lease_token,
            "验收主动释放租约".to_owned(),
        )
        .expect("lease must release");

    let identical_preview = plan_service
        .preview_publish(fixture.home.clone(), blocked_candidate.id)
        .expect("released preview must load");
    assert!(identical_preview.publishable);
    assert!(identical_preview.carried_completed >= 1);
    let second_workspace = plan_service
        .publish_confirmed(fixture.home.clone(), blocked_candidate.id, true)
        .expect("identical plan must publish after release");
    let second_plan_id = active_plan_id(&second_workspace);
    assert_eq!(
        view_status(&second_workspace, &foundation_key),
        WorkItemStatus::Done
    );
    assert!(
        second_workspace
            .work_items
            .iter()
            .all(|view| view.work_item.plan_revision_id == second_plan_id)
    );

    let changed_candidate = plan_service
        .generate_draft(fixture.home.clone())
        .expect("changed draft must generate")
        .draft
        .expect("changed draft must exist");
    let mut changed_content = changed_candidate.content;
    let contract_key = changed_content
        .tasks
        .iter()
        .find(|task| task.task_key.starts_with("contracts:"))
        .expect("contract task must exist")
        .task_key
        .clone();
    changed_content
        .tasks
        .iter_mut()
        .find(|task| task.task_key == contract_key)
        .expect("contract task must exist")
        .goal
        .push_str("；新增幂等键与错误契约修订");
    changed_content.tasks.push(manual_review_task());
    let changed_draft = plan_service
        .save_draft(fixture.home.clone(), changed_content)
        .expect("changed draft must save")
        .draft
        .expect("changed draft must remain");
    let changed_preview = plan_service
        .preview_publish(fixture.home.clone(), changed_draft.id)
        .expect("changed preview must load");
    assert!(changed_preview.carried_completed >= 1);
    assert!(
        changed_preview
            .changed
            .iter()
            .any(|task| task.task_key == contract_key
                && task.next_status == Some(WorkItemStatus::Reopened))
    );
    assert!(
        changed_preview
            .added
            .iter()
            .any(|task| task.task_key == "manual:launch-review")
    );
    let third_workspace = plan_service
        .publish_confirmed(fixture.home.clone(), changed_draft.id, true)
        .expect("changed plan must publish");
    let third_plan_id = active_plan_id(&third_workspace);
    assert_eq!(
        view_status(&third_workspace, &foundation_key),
        WorkItemStatus::Done
    );
    assert_eq!(
        view_status(&third_workspace, "manual:launch-review"),
        WorkItemStatus::Ready
    );
    let manual_item = third_workspace
        .work_items
        .iter()
        .find(|view| view.task.task_key == "manual:launch-review")
        .expect("manual task must be active")
        .work_item
        .clone();

    let removal_candidate = plan_service
        .generate_draft(fixture.home.clone())
        .expect("removal draft must generate")
        .draft
        .expect("removal draft must exist");
    let removal_preview = plan_service
        .preview_publish(fixture.home.clone(), removal_candidate.id)
        .expect("removal preview must load");
    assert_eq!(removal_preview.cancelled, 1);
    assert!(removal_preview.removed.iter().any(|task| {
        task.task_key == "manual:launch-review"
            && task.next_status == Some(WorkItemStatus::Cancelled)
    }));
    let fourth_workspace = plan_service
        .publish_confirmed(fixture.home.clone(), removal_candidate.id, true)
        .expect("removal plan must publish");
    let fourth_plan_id = active_plan_id(&fourth_workspace);
    assert!(
        fourth_workspace
            .work_items
            .iter()
            .all(|view| view.task.task_key != "manual:launch-review")
    );
    assert_eq!(
        view_status(&fourth_workspace, &foundation_key),
        WorkItemStatus::Done
    );
    assert_ne!(third_plan_id, fourth_plan_id);

    let database_path = fixture.home.join(".coldstart").join("state.db");
    let database = Database::open(&database_path).expect("database must reopen");
    assert_eq!(
        database
            .latest_plan(&fixture.project_id, coldstart_core::PlanStatus::Published)
            .expect("active plan must load")
            .expect("active plan must exist")
            .id,
        fourth_plan_id
    );
    assert!(
        database
            .list_plans(&fixture.project_id)
            .expect("plan history must load")
            .len()
            >= 4
    );
    assert!(
        database
            .list_work_reports(&fixture.project_id)
            .expect("reports must load")
            .iter()
            .any(|report| report.accepted)
    );
    assert!(
        database
            .list_git_syncs(&fixture.project_id)
            .expect("Git syncs must load")
            .iter()
            .any(|sync| sync.status == GitSyncStatus::Pushed)
    );
    assert_eq!(
        database
            .list_repositories(&fixture.project_id)
            .expect("repositories must load")
            .len(),
        1
    );
    assert_eq!(
        database
            .list_credentials(&fixture.project_id)
            .expect("credentials must load")
            .len(),
        1
    );
    assert!(
        database
            .list_capability_states(&fixture.project_id)
            .expect("capability states must load")
            .iter()
            .any(|state| state.status == CapabilityStatus::Configured)
    );
    assert_eq!(
        database
            .load_work_item(&manual_item.id)
            .expect("historical work item must load")
            .expect("historical work item must remain")
            .status,
        WorkItemStatus::Cancelled
    );
    drop(database);

    let restarted_plan = PlanService::with_dependencies(secrets.clone(), GitAdapter::new());
    let restarted_workspace = restarted_plan
        .load_workspace(fixture.home.clone())
        .expect("workspace must survive restart");
    assert_eq!(active_plan_id(&restarted_workspace), fourth_plan_id);
    assert_eq!(
        view_status(&restarted_workspace, &foundation_key),
        WorkItemStatus::Done
    );
    let restarted_integrations =
        IntegrationService::with_dependencies(secrets, GitAdapter::new(), None);
    let restarted_integration_workspace = restarted_integrations
        .load_workspace(fixture.home.clone())
        .expect("integration workspace must survive restart");
    assert_eq!(restarted_integration_workspace.repositories.len(), 1);
    assert_eq!(
        restarted_integration_workspace.verification_commands[0].commands,
        vec![verification_command()]
    );
    assert_eq!(restarted_integration_workspace.credentials.len(), 1);
    assert!(
        restarted_integration_workspace
            .git_syncs
            .iter()
            .any(|sync| sync.status == GitSyncStatus::Pushed)
    );
    assert!(
        restarted_integration_workspace
            .capabilities
            .iter()
            .any(|state| state.capability_id == capability_id
                && state.status == CapabilityStatus::Configured)
    );
    assert!(
        !serde_json::to_string(&restarted_workspace)
            .expect("workspace must serialize")
            .contains(OSS_SECRET)
    );
    assert!(
        !serde_json::to_string(&restarted_workspace)
            .expect("workspace must serialize")
            .contains(&lease_token)
    );

    let values = scan_text_columns(&database_path);
    assert!(
        values.len() > 20,
        "security scan must cover persisted tables"
    );
    assert!(
        values
            .iter()
            .all(|(_, _, value)| !value.contains(OSS_SECRET)),
        "a secret leaked into SQLite"
    );
    assert!(
        values
            .iter()
            .all(|(_, _, value)| !value.contains(&lease_token)),
        "a raw lease token leaked into SQLite"
    );
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn scan_text_columns(database_path: &Path) -> Vec<(String, String, String)> {
    let connection = Connection::open(database_path).expect("SQLite database must open");
    let mut tables_statement = connection
        .prepare(
            "SELECT name FROM sqlite_master
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
             ORDER BY name",
        )
        .expect("table query must prepare");
    let tables = tables_statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("table query must run")
        .collect::<Result<Vec<_>, _>>()
        .expect("table names must load");
    drop(tables_statement);

    let mut values = Vec::new();
    for table in tables {
        let quoted_table = quote_identifier(&table);
        let mut columns_statement = connection
            .prepare(&format!("PRAGMA table_info({quoted_table})"))
            .expect("column query must prepare");
        let columns = columns_statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(1)?, row.get::<_, String>(2)?))
            })
            .expect("column query must run")
            .collect::<Result<Vec<_>, _>>()
            .expect("column names must load");
        drop(columns_statement);
        for (column, column_type) in columns {
            if !column_type.to_ascii_uppercase().contains("TEXT") {
                continue;
            }
            let quoted_column = quote_identifier(&column);
            let query = format!(
                "SELECT CAST({quoted_column} AS TEXT) FROM {quoted_table} WHERE {quoted_column} IS NOT NULL"
            );
            let mut value_statement = connection
                .prepare(&query)
                .expect("column value query must prepare");
            let column_values = value_statement
                .query_map([], |row| row.get::<_, String>(0))
                .expect("column value query must run")
                .collect::<Result<Vec<_>, _>>()
                .expect("column values must load");
            for value in column_values {
                values.push((table.clone(), column.clone(), value));
            }
        }
    }
    values
}
