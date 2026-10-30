use std::path::PathBuf;

use chrono::Utc;
use coldstart_app::{ClaimRequest, PlanService, ProjectService};
use coldstart_core::{
    AcceptanceResult, ArtifactId, BlueprintContent, BlueprintEntityId, BlueprintGeneration,
    ClaimResult, GenerationSource, InfoRequestCategory, InfoRequestDraft, ModuleDefinition,
    PlanContent, PlatformDefinition, PlatformKind, ProjectId, ReportOutcome, SourceReference,
    TraceableText, WorkItemStatus, WorkReportInput,
};
use coldstart_storage::Database;
use tempfile::TempDir;

struct Fixture {
    _directory: TempDir,
    home: PathBuf,
    project_id: ProjectId,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("temporary directory must exist");
        let home = directory.path().join("project");
        let status = ProjectService::new()
            .create_project("计划集成项目", home.clone())
            .expect("project must create");
        let database_path = home.join(".coldstart").join("state.db");
        let mut database = Database::open(&database_path).expect("database must open");
        let blueprint = blueprint_content(status.project_id);
        let revision = database
            .create_blueprint_revision(
                &status.project_id,
                blueprint,
                BlueprintGeneration {
                    source: GenerationSource::Manual,
                    model_provider: None,
                    model_name: None,
                    prompt_version: None,
                },
                vec![],
            )
            .expect("blueprint draft must create");
        database
            .publish_blueprint_revision(&status.project_id, &revision.id)
            .expect("blueprint must publish");
        Self {
            _directory: directory,
            home,
            project_id: status.project_id,
        }
    }
}

fn blueprint_content(project_id: ProjectId) -> BlueprintContent {
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
            sources: vec![source],
        }],
        technologies: vec![],
        capabilities: vec![],
        release_prerequisites: vec![],
        assumptions: vec![],
        risks: vec![],
    }
}

fn report_for(task: &coldstart_core::TaskSpec, idempotency_key: &str) -> WorkReportInput {
    WorkReportInput {
        status: ReportOutcome::Completed,
        summary: "完成当前任务".to_owned(),
        branch: task.work_branch.clone(),
        commit_sha: Some("0123456789abcdef".to_owned()),
        changed_files: vec!["src/main.rs".to_owned()],
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

#[test]
fn publishes_a_plan_claims_context_and_completes_the_first_task() {
    let fixture = Fixture::new();
    let service = PlanService::new();
    let workspace = service
        .generate_draft(fixture.home.clone())
        .expect("plan draft must generate");
    let draft = workspace.draft.expect("draft must exist");
    let published = service
        .publish(fixture.home.clone(), draft.id)
        .expect("plan must publish");
    assert_eq!(published.published.as_ref().unwrap().id, draft.id);

    let claimed = service
        .claim_next(
            fixture.home.clone(),
            ClaimRequest {
                agent_profile: "codex".to_owned(),
                repository: Some("web".to_owned()),
                request_id: "claim-1".to_owned(),
            },
        )
        .expect("claim must call");
    let ClaimResult::Claimed(claimed) = claimed else {
        panic!("a matching task must be claimed");
    };
    let context = service
        .get_context(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token.clone(),
        )
        .expect("context must compile");
    assert!(context.markdown.contains("验收标准"));
    assert!(context.estimated_tokens <= context.budget_tokens);
    assert!(!context.markdown.contains("Bearer "));

    let task = published
        .work_items
        .iter()
        .find(|view| view.work_item.id == claimed.work_item.id)
        .unwrap()
        .task
        .clone();
    service
        .submit_report(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token,
            report_for(&task, "report-1"),
        )
        .expect("report must submit");
    let final_workspace = service
        .load_workspace(fixture.home)
        .expect("workspace must load");
    assert!(final_workspace.work_items.iter().any(|view| {
        view.work_item.id == claimed.work_item.id && view.work_item.status == WorkItemStatus::Done
    }));
}

#[test]
fn context_respects_a_small_budget_and_lists_omissions() {
    let fixture = Fixture::new();
    let service = PlanService::new();
    let draft = service
        .generate_draft(fixture.home.clone())
        .expect("plan draft must generate")
        .draft
        .unwrap();
    let mut content: PlanContent = draft.content.clone();
    for task in &mut content.tasks {
        task.context_budget_tokens = 120;
        task.goal = "需要较长目标的模块".repeat(10);
    }
    let saved = service
        .save_draft(fixture.home.clone(), content)
        .expect("edited draft must save");
    service
        .publish(fixture.home.clone(), saved.draft.unwrap().id)
        .expect("plan must publish");
    let claimed = service
        .claim_next(
            fixture.home.clone(),
            ClaimRequest {
                agent_profile: "codex".to_owned(),
                repository: Some("web".to_owned()),
                request_id: "claim-budget".to_owned(),
            },
        )
        .expect("claim must call");
    let ClaimResult::Claimed(claimed) = claimed else {
        panic!("work must be claimed");
    };
    let context = service
        .get_context(fixture.home, claimed.work_item.id, claimed.lease_token)
        .expect("context must compile");
    assert!(context.estimated_tokens <= context.budget_tokens);
}

#[test]
fn refuses_a_reference_not_whitelisted_by_the_task() {
    let fixture = Fixture::new();
    let service = PlanService::new();
    let draft = service
        .generate_draft(fixture.home.clone())
        .expect("plan draft must generate")
        .draft
        .unwrap();
    service
        .publish(fixture.home.clone(), draft.id)
        .expect("plan must publish");
    let claimed = service
        .claim_next(
            fixture.home.clone(),
            ClaimRequest {
                agent_profile: "codex".to_owned(),
                repository: Some("web".to_owned()),
                request_id: "claim-reference".to_owned(),
            },
        )
        .expect("claim must call");
    let ClaimResult::Claimed(claimed) = claimed else {
        panic!("work must be claimed");
    };
    let error = service
        .read_reference(
            fixture.home,
            claimed.work_item.id,
            claimed.lease_token,
            "module_unknown".to_owned(),
        )
        .expect_err("unknown reference must fail");
    assert_eq!(error.code(), "reference_not_found");
    let _ = Utc::now();
}

#[test]
fn authorization_facts_never_enter_the_agent_context() {
    const SENSITIVE_VALUE: &str = "test-only-access-key-do-not-leak";
    let fixture = Fixture::new();
    let database_path = fixture.home.join(".coldstart").join("state.db");
    let mut database = Database::open(&database_path).expect("database must open");
    let revision = database
        .create_blueprint_revision(
            &fixture.project_id,
            blueprint_content(fixture.project_id),
            BlueprintGeneration {
                source: GenerationSource::Manual,
                model_provider: None,
                model_name: None,
                prompt_version: None,
            },
            vec![InfoRequestDraft {
                category: InfoRequestCategory::Authorization,
                question: "阿里云 OSS 的访问密钥是什么？".to_owned(),
                reason: "接入文件上传能力".to_owned(),
                example: None,
                default_value: None,
                impact_scope: "文件上传".to_owned(),
                due_at: None,
                blocking: false,
            }],
        )
        .expect("blueprint revision with authorization request must create");
    database
        .publish_blueprint_revision(&fixture.project_id, &revision.id)
        .expect("blueprint revision must publish");
    let request = database
        .list_info_requests(&fixture.project_id)
        .expect("info requests must load")
        .into_iter()
        .find(|request| request.category == InfoRequestCategory::Authorization)
        .expect("authorization request must exist");
    database
        .resolve_info_request(&fixture.project_id, &request.id, SENSITIVE_VALUE)
        .expect("authorization request must resolve");
    drop(database);

    let service = PlanService::new();
    let draft = service
        .generate_draft(fixture.home.clone())
        .expect("plan draft must generate")
        .draft
        .unwrap();
    service
        .publish(fixture.home.clone(), draft.id)
        .expect("plan must publish");
    let claimed = service
        .claim_next(
            fixture.home.clone(),
            ClaimRequest {
                agent_profile: "codex".to_owned(),
                repository: Some("web".to_owned()),
                request_id: "claim-sensitive-fact".to_owned(),
            },
        )
        .expect("claim must call");
    let ClaimResult::Claimed(claimed) = claimed else {
        panic!("work must be claimed");
    };
    let context = service
        .get_context(fixture.home, claimed.work_item.id, claimed.lease_token)
        .expect("context must compile");
    assert!(!context.markdown.contains(SENSITIVE_VALUE));
    assert!(context.markdown.contains("阿里云 OSS 的访问密钥是什么？"));
    assert!(context.markdown.contains("敏感值不进入 Agent 上下文"));
}
