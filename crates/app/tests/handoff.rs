use std::{path::PathBuf, sync::Arc};

use coldstart_app::{ClaimRequest, ContextHandoffService, PlanService, ProjectService};
use coldstart_core::{
    AcceptanceResult, ArtifactId, BlueprintContent, BlueprintEntityId, BlueprintGeneration,
    CapabilityObligation, ClaimResult, GenerationSource, InfoRequestCategory, InfoRequestDraft,
    ModuleDefinition, PlatformDefinition, PlatformKind, ProjectId, ReportOutcome, SourceReference,
    TraceableText, WorkReportInput,
};
use coldstart_storage::Database;
use tempfile::TempDir;

#[path = "support/mod.rs"]
mod support;

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
            .create_project("接力测试项目", home.clone())
            .expect("project must create");
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
                vec![
                    InfoRequestDraft {
                        category: InfoRequestCategory::Authorization,
                        question: "生产环境 OSS AccessKey 是什么？".to_owned(),
                        reason: "上传订单附件需要对象存储".to_owned(),
                        example: None,
                        default_value: None,
                        impact_scope: "订单附件上传".to_owned(),
                        due_at: None,
                        blocking: false,
                    },
                    InfoRequestDraft {
                        category: InfoRequestCategory::Decision,
                        question: "订单是否允许部分退款？".to_owned(),
                        reason: "影响退款接口和结算规则".to_owned(),
                        example: None,
                        default_value: None,
                        impact_scope: "退款模块".to_owned(),
                        due_at: None,
                        blocking: false,
                    },
                ],
            )
            .expect("blueprint draft must create");
        database
            .publish_blueprint_revision(&status.project_id, &revision.id)
            .expect("blueprint must publish");

        let requests = database
            .list_info_requests(&status.project_id)
            .expect("info requests must load");
        let authorization = requests
            .iter()
            .find(|request| request.category == InfoRequestCategory::Authorization)
            .expect("authorization request must exist");
        database
            .resolve_info_request(
                &status.project_id,
                &authorization.id,
                "TOP_SECRET_FACT_VALUE",
            )
            .expect("authorization fact must resolve");
        let decision = requests
            .iter()
            .find(|request| request.category == InfoRequestCategory::Decision)
            .expect("decision request must exist");
        database
            .resolve_info_request(&status.project_id, &decision.id, "允许按商品行部分退款")
            .expect("decision fact must resolve");

        support::configure_repository(&home, status.project_id, "web", true);
        Self {
            _directory: directory,
            home,
            project_id: status.project_id,
        }
    }

    fn service(&self) -> PlanService {
        support::service(
            Arc::new(support::FakePlanGit::passing()),
            Arc::new(support::ScriptedExecutor::default()),
        )
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
            sources: vec![source.clone()],
        }],
        technologies: vec![],
        capabilities: vec![CapabilityObligation {
            id: BlueprintEntityId::derive(&project_id, "capability", "阿里云 OSS"),
            name: "阿里云 OSS".to_owned(),
            provider: "aliyun_oss".to_owned(),
            description: "上传订单附件".to_owned(),
            sources: vec![source],
        }],
        release_prerequisites: vec![],
        assumptions: vec![],
        risks: vec![],
    }
}

fn publish_plan(service: &PlanService, fixture: &Fixture) -> coldstart_app::PlanWorkspace {
    let draft = service
        .generate_draft(fixture.home.clone())
        .expect("plan draft must generate")
        .draft
        .expect("draft must exist");
    service
        .publish(fixture.home.clone(), draft.id)
        .expect("plan must publish")
}

fn claim_one(
    service: &PlanService,
    fixture: &Fixture,
    request_id: &str,
) -> coldstart_core::ClaimedWork {
    match service
        .claim_next(
            fixture.home.clone(),
            ClaimRequest {
                agent_profile: "codex".to_owned(),
                repository: Some("web".to_owned()),
                request_id: request_id.to_owned(),
            },
        )
        .expect("claim must succeed")
    {
        ClaimResult::Claimed(claimed) => claimed,
        other => panic!("work must be claimed, got {other:?}"),
    }
}

fn task_for(
    workspace: &coldstart_app::PlanWorkspace,
    work_id: coldstart_core::WorkItemId,
) -> coldstart_core::TaskSpec {
    workspace
        .work_items
        .iter()
        .find(|view| view.work_item.id == work_id)
        .expect("work item must exist")
        .task
        .clone()
}

fn report_for(task: &coldstart_core::TaskSpec, idempotency_key: &str) -> WorkReportInput {
    WorkReportInput {
        status: ReportOutcome::Completed,
        summary: "完成当前任务，调试 token=super-secret-token".to_owned(),
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
        known_issues: vec!["api_key=should-not-leak".to_owned()],
        follow_up_items: vec!["补齐退款异常分支".to_owned()],
        contract_hash: task.contract_hash(),
        idempotency_key: idempotency_key.to_owned(),
    }
}

#[test]
fn project_brief_exposes_safe_versions_reports_and_memory() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let published = publish_plan(&service, &fixture);
    let claimed = claim_one(&service, &fixture, "brief-claim");
    let task = task_for(&published, claimed.work_item.id);
    service
        .submit_report(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token.clone(),
            report_for(&task, "brief-report"),
        )
        .expect("report must complete");

    let brief = ContextHandoffService::new()
        .project_brief(fixture.home.clone())
        .expect("project brief must load");
    let serialized = serde_json::to_string(&brief).expect("brief must serialize");

    assert_eq!(brief.project.id, fixture.project_id);
    assert_eq!(brief.project.name, "接力测试项目");
    assert_eq!(brief.blueprint.as_ref().unwrap().module_count, 1);
    assert_eq!(
        brief.plan.as_ref().unwrap().task_count,
        published.work_items.len()
    );
    assert!(
        brief
            .repositories
            .iter()
            .any(|repository| repository.key == "web")
    );
    assert!(brief.recent_memory.len() >= 10);
    assert!(
        brief
            .recent_completed_work
            .iter()
            .any(|work| work.work_item_id == claimed.work_item.id)
    );
    let completed = brief
        .recent_completed_work
        .iter()
        .find(|work| work.work_item_id == claimed.work_item.id)
        .unwrap();
    let report = completed.latest_report.as_ref().expect("report must exist");
    assert!(report.summary.contains("***"));
    assert!(!report.summary.contains("super-secret-token"));
    assert!(
        report
            .known_issues
            .iter()
            .all(|issue| !issue.contains("should-not-leak"))
    );
    assert!(
        report
            .follow_up_items
            .contains(&"补齐退款异常分支".to_owned())
    );

    let authorization_fact = brief
        .confirmed_facts
        .iter()
        .find(|fact| fact.question.contains("AccessKey"))
        .expect("authorization fact must exist");
    assert_eq!(authorization_fact.value, "已登记（敏感值不进入 MCP）");
    assert!(brief.confirmed_facts.iter().any(|fact| {
        fact.question.contains("部分退款") && fact.value.contains("按商品行")
    }));

    assert!(!serialized.contains("TOP_SECRET_FACT_VALUE"));
    assert!(!serialized.contains("super-secret-token"));
    assert!(!serialized.contains("should-not-leak"));
    assert!(!serialized.contains(&claimed.lease_token));
    assert!(!serialized.contains("https://example.invalid"));
    assert!(!serialized.contains(&fixture.home.to_string_lossy().into_owned()));
}

#[test]
fn work_handoff_includes_related_completed_work_and_requires_a_lease() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let published = publish_plan(&service, &fixture);
    let first = claim_one(&service, &fixture, "handoff-first");
    let first_task = task_for(&published, first.work_item.id);
    let first_task_key = first_task.task_key.clone();
    service
        .submit_report(
            fixture.home.clone(),
            first.work_item.id,
            first.lease_token,
            report_for(&first_task, "handoff-report"),
        )
        .expect("first report must complete");

    let second = claim_one(&service, &fixture, "handoff-second");
    let lease_token = second.lease_token.clone();
    let handoff = ContextHandoffService::new()
        .work_handoff(
            fixture.home.clone(),
            second.work_item.id,
            second.lease_token,
        )
        .expect("handoff must load");
    let serialized = serde_json::to_string(&handoff).expect("handoff must serialize");

    assert_eq!(handoff.current_work.work_item_id, second.work_item.id);
    assert!(!handoff.current_work.task_key.is_empty());
    assert!(
        handoff
            .related_work
            .iter()
            .any(|work| work.task_key == first_task_key)
    );
    let previous = handoff
        .related_work
        .iter()
        .find(|work| work.task_key == first_task_key)
        .unwrap();
    assert!(
        previous
            .latest_report
            .as_ref()
            .expect("previous report must exist")
            .follow_up_items
            .contains(&"补齐退款异常分支".to_owned())
    );
    assert!(!serialized.contains(&lease_token));
    assert!(!serialized.contains("super-secret-token"));
    assert!(!serialized.contains("https://example.invalid"));

    let invalid = ContextHandoffService::new()
        .work_handoff(
            fixture.home,
            second.work_item.id,
            "invalid-lease".to_owned(),
        )
        .expect_err("invalid lease must be rejected");
    assert_eq!(invalid.code(), "lease_invalid");
}

#[test]
fn project_brief_handles_a_project_without_a_published_plan() {
    let directory = tempfile::tempdir().expect("temporary directory must exist");
    let home = directory.path().join("project");
    let status = ProjectService::new()
        .create_project("空项目", home.clone())
        .expect("project must create");
    let brief = ContextHandoffService::new()
        .project_brief(home)
        .expect("brief must load");

    assert_eq!(brief.project.id, status.project_id);
    assert!(brief.blueprint.is_none());
    assert!(brief.plan.is_none());
    assert!(brief.current_work.is_empty());
    assert!(brief.recent_completed_work.is_empty());
    assert!(brief.repositories.is_empty());
}
