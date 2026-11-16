use std::{fs, path::PathBuf, sync::Arc};

use chrono::Utc;
use coldstart_adapters::{GitAdapter, MemorySecretStore};
use coldstart_app::{ClaimRequest, IntegrationService, PlanService, ProjectService};
use coldstart_core::{
    AcceptanceCriterion, AcceptancePolicy, AcceptanceResult, ArtifactFormat, ArtifactId,
    ArtifactImage, ArtifactMetadata, BlueprintContent, BlueprintEntityId, BlueprintGeneration,
    ClaimFailure, ClaimResult, DocumentAnchor, GenerationSource, InfoRequestCategory,
    InfoRequestDraft, ModuleDefinition, PlanContent, PlatformDefinition, PlatformKind, ProjectId,
    ReportOutcome, SourceReference, TaskKind, TaskSpec, TraceableText, VerificationStatus,
    WorkItemStatus, WorkReportInput,
};
use coldstart_storage::Database;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

#[path = "support/mod.rs"]
mod support;

struct Fixture {
    _directory: TempDir,
    home: PathBuf,
    project_id: ProjectId,
    artifact_id: ArtifactId,
    unrelated_artifact_id: ArtifactId,
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
        let artifact_id = seed_artifact(
            &home,
            status.project_id,
            &[
                ("section_1", "image_1", PNG_BYTES),
                (
                    "section_2",
                    "image_2",
                    b"\x89PNG\r\n\x1a\nunrelated-section-image",
                ),
            ],
        );
        let unrelated_artifact_id = seed_artifact(
            &home,
            status.project_id,
            &[(
                "section_1",
                "image_1",
                b"\x89PNG\r\n\x1a\nunrelated-document-image",
            )],
        );
        let blueprint = blueprint_content(status.project_id, artifact_id);
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
        support::configure_repository(&home, status.project_id, "web", true);
        Self {
            _directory: directory,
            home,
            project_id: status.project_id,
            artifact_id,
            unrelated_artifact_id,
        }
    }

    fn service(&self) -> PlanService {
        support::service(
            Arc::new(support::FakePlanGit::passing()),
            Arc::new(support::ScriptedExecutor::default()),
        )
    }
}

const PNG_BYTES: &[u8] = b"\x89PNG\r\n\x1a\ncoldstart-image";

fn seed_artifact(
    home: &std::path::Path,
    project_id: ProjectId,
    images: &[(&str, &str, &[u8])],
) -> ArtifactId {
    let artifact_id = ArtifactId::new();
    let artifact = ArtifactMetadata {
        id: artifact_id,
        project_id,
        file_name: "requirements.docx".to_owned(),
        format: ArtifactFormat::Docx,
        content_hash: "artifact-content-hash".to_owned(),
        text_hash: "artifact-text-hash".to_owned(),
        byte_size: 512,
        import_source: "requirements.docx".to_owned(),
        imported_at: Utc::now(),
    };
    let anchors = images
        .iter()
        .map(|(anchor_id, _, _)| DocumentAnchor {
            id: (*anchor_id).to_owned(),
            label: format!("章节 {anchor_id}"),
            text: format!("章节 {anchor_id} 的需求正文"),
        })
        .collect::<Vec<_>>();
    let artifact_images = images
        .iter()
        .map(|(anchor_id, image_id, bytes)| ArtifactImage {
            id: (*image_id).to_owned(),
            anchor_id: (*anchor_id).to_owned(),
            file_name: format!("{image_id}.png"),
            mime_type: "image/png".to_owned(),
            byte_size: bytes.len() as u64,
            sha256: format!("{:x}", Sha256::digest(bytes)),
            relative_path: format!("images/{image_id}.png"),
        })
        .collect::<Vec<_>>();
    let mut database =
        Database::open(&home.join(".coldstart").join("state.db")).expect("database must open");
    database
        .insert_artifact_with_images(&artifact, &anchors, &artifact_images)
        .expect("artifact must insert");
    let images_directory = home
        .join(".coldstart")
        .join("artifacts")
        .join(artifact_id.to_string())
        .join("images");
    fs::create_dir_all(&images_directory).expect("image directory must create");
    for ((_, image_id, bytes), metadata) in images.iter().zip(&artifact_images) {
        assert_eq!(metadata.id, *image_id);
        fs::write(images_directory.join(format!("{image_id}.png")), bytes)
            .expect("image must write");
    }
    artifact_id
}

fn blueprint_content(project_id: ProjectId, artifact_id: ArtifactId) -> BlueprintContent {
    let source = SourceReference {
        artifact_id,
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

fn publish_and_claim(
    service: &PlanService,
    fixture: &Fixture,
    request_id: &str,
) -> coldstart_core::ClaimedWork {
    let draft = service
        .generate_draft(fixture.home.clone())
        .expect("plan draft must generate")
        .draft
        .expect("draft must exist");
    service
        .publish(fixture.home.clone(), draft.id)
        .expect("plan must publish");
    match service
        .claim_next(
            fixture.home.clone(),
            ClaimRequest {
                agent_profile: "codex".to_owned(),
                repository: Some("web".to_owned()),
                request_id: request_id.to_owned(),
            },
        )
        .expect("claim must call")
    {
        ClaimResult::Claimed(claimed) => claimed,
        other => panic!("work must be claimed, got {other:?}"),
    }
}

fn task_for(
    service: &PlanService,
    home: &std::path::Path,
    task_key: &str,
) -> coldstart_core::TaskSpec {
    service
        .load_workspace(home.to_path_buf())
        .expect("workspace must load")
        .work_items
        .into_iter()
        .find(|view| view.task.task_key == task_key)
        .expect("task must exist")
        .task
}

#[test]
fn publishes_a_plan_claims_context_and_completes_the_first_task() {
    let fixture = Fixture::new();
    let service = fixture.service();
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
    let service = fixture.service();
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
    let service = fixture.service();
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

fn publish_and_claim_image_task(
    service: &PlanService,
    fixture: &Fixture,
    request_id: &str,
) -> coldstart_core::ClaimedWork {
    let module_id = BlueprintEntityId::derive(&fixture.project_id, "module", "Orders");
    let platform_id = BlueprintEntityId::derive(&fixture.project_id, "platform", "Web");
    let task = TaskSpec {
        task_key: "image-slice".to_owned(),
        title: "订单图片原型任务".to_owned(),
        kind: TaskKind::AgentCoding,
        goal: "按订单原型实现任务图片展示".to_owned(),
        scope: vec!["订单模块".to_owned()],
        out_of_scope: vec!["不实现其他模块".to_owned()],
        module_id: Some(module_id.clone()),
        platform_id: Some(platform_id.clone()),
        repository_key: Some("web".to_owned()),
        base_ref: Some("main".to_owned()),
        work_branch: Some("codex/image-slice".to_owned()),
        allowed_paths: vec![".".to_owned()],
        context_refs: vec![module_id, platform_id],
        acceptance_criteria: vec![AcceptanceCriterion {
            id: "implementation".to_owned(),
            text: "订单图片原型对应界面已实现".to_owned(),
            required_evidence: false,
        }],
        acceptance_policy: AcceptancePolicy::AgentReport,
        evidence_requirements: vec![],
        priority: 700,
        critical_path: false,
        context_budget_tokens: 12_000,
        resource_keys: vec!["web".to_owned()],
        dependencies: vec![],
    };
    let draft = service
        .save_draft(
            fixture.home.clone(),
            PlanContent {
                tasks: vec![task],
                warnings: vec![],
            },
        )
        .expect("image task draft must save")
        .draft
        .expect("image task draft must exist");
    service
        .publish(fixture.home.clone(), draft.id)
        .expect("image task plan must publish");
    match service
        .claim_next(
            fixture.home.clone(),
            ClaimRequest {
                agent_profile: "codex".to_owned(),
                repository: Some("web".to_owned()),
                request_id: request_id.to_owned(),
            },
        )
        .expect("image task claim must call")
    {
        ClaimResult::Claimed(claimed) => claimed,
        other => panic!("image task must be claimed, got {other:?}"),
    }
}

#[test]
fn only_lists_images_referenced_by_the_task_blueprint_anchors() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let claimed = publish_and_claim_image_task(&service, &fixture, "claim-task-images");

    let images = service
        .list_task_images(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token.clone(),
        )
        .expect("task images must list");
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].artifact_id, fixture.artifact_id);
    assert_eq!(images[0].image_id, "image_1");
    assert_eq!(images[0].anchor_id, "section_1");
    assert_eq!(images[0].anchor_label, "章节 section_1");
    assert_eq!(images[0].artifact_file_name, "requirements.docx");
    assert_eq!(images[0].mime_type, "image/png");
    assert_eq!(images[0].byte_size, PNG_BYTES.len() as u64);

    let content = service
        .read_task_image(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token.clone(),
            fixture.artifact_id,
            "image_1".to_owned(),
        )
        .expect("authorized image must read");
    assert_eq!(content.image, images[0]);
    assert_eq!(content.bytes, PNG_BYTES);
}

#[test]
fn task_images_exclude_other_sections_and_unrelated_documents() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let claimed = publish_and_claim_image_task(&service, &fixture, "claim-hidden-images");
    let images = service
        .list_task_images(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token.clone(),
        )
        .expect("task images must list");
    assert!(!images.iter().any(|image| image.image_id == "image_2"));
    assert!(
        !images
            .iter()
            .any(|image| image.artifact_id == fixture.unrelated_artifact_id)
    );

    let other_section = service
        .read_task_image(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token.clone(),
            fixture.artifact_id,
            "image_2".to_owned(),
        )
        .expect_err("image from another section must not be readable");
    assert_eq!(other_section.code(), "requirement_image_not_found");

    let other_document = service
        .read_task_image(
            fixture.home,
            claimed.work_item.id,
            claimed.lease_token,
            fixture.unrelated_artifact_id,
            "image_1".to_owned(),
        )
        .expect_err("image from an unrelated document must not be readable");
    assert_eq!(other_document.code(), "requirement_image_not_found");
}

#[test]
fn task_images_require_a_valid_lease() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let claimed = publish_and_claim_image_task(&service, &fixture, "claim-invalid-image-lease");

    let error = service
        .list_task_images(
            fixture.home.clone(),
            claimed.work_item.id,
            "invalid-lease".to_owned(),
        )
        .expect_err("invalid lease must not list task images");
    assert_eq!(error.code(), "lease_invalid");

    let error = service
        .read_task_image(
            fixture.home,
            claimed.work_item.id,
            "invalid-lease".to_owned(),
            fixture.artifact_id,
            "image_1".to_owned(),
        )
        .expect_err("invalid lease must not read a task image");
    assert_eq!(error.code(), "lease_invalid");
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
            blueprint_content(fixture.project_id, fixture.artifact_id),
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

    let service = fixture.service();
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

#[test]
fn replanning_requires_a_preview_and_explicit_confirmation() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let first = service
        .generate_draft(fixture.home.clone())
        .expect("first plan draft must generate")
        .draft
        .expect("first draft must exist");
    service
        .publish(fixture.home.clone(), first.id)
        .expect("first plan must publish");

    let second = service
        .generate_draft(fixture.home.clone())
        .expect("replacement draft must generate")
        .draft
        .expect("replacement draft must exist");
    let preview = service
        .preview_publish(fixture.home.clone(), second.id)
        .expect("replacement preview must load");
    assert_eq!(preview.base_revision_id, Some(first.id));
    assert!(preview.publishable);
    assert!(preview.added.is_empty());
    assert_eq!(preview.unchanged.len(), second.content.tasks.len());

    let error = service
        .publish_confirmed(fixture.home.clone(), second.id, false)
        .expect_err("unconfirmed replacement must fail");
    assert_eq!(error.code(), "replan_confirmation_required");

    let workspace = service
        .publish_confirmed(fixture.home.clone(), second.id, true)
        .expect("confirmed replacement must publish");
    assert_eq!(
        workspace.published.expect("published plan must exist").id,
        second.id
    );
}

#[test]
fn replan_workspace_projects_only_the_active_plan() {
    let fixture = Fixture::new();
    let secrets = Arc::new(MemorySecretStore::default());
    let service = support::service_with_secrets(
        secrets.clone(),
        Arc::new(support::FakePlanGit::passing()),
        Arc::new(support::ScriptedExecutor::default()),
    );
    let first = service
        .generate_draft(fixture.home.clone())
        .expect("first plan draft must generate")
        .draft
        .expect("first draft must exist");
    let first_workspace = service
        .publish(fixture.home.clone(), first.id)
        .expect("first plan must publish");
    let claimed = service
        .claim_next(
            fixture.home.clone(),
            ClaimRequest {
                agent_profile: "codex".to_owned(),
                repository: Some("web".to_owned()),
                request_id: "replan-projection-claim".to_owned(),
            },
        )
        .expect("claim must call");
    let ClaimResult::Claimed(claimed) = claimed else {
        panic!("work must be claimed");
    };
    let task = first_workspace
        .work_items
        .iter()
        .find(|view| view.work_item.id == claimed.work_item.id)
        .expect("claimed task must exist")
        .task
        .clone();
    service
        .submit_report(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token,
            report_for(&task, "replan-projection-report"),
        )
        .expect("report must submit");

    let second = service
        .generate_draft(fixture.home.clone())
        .expect("replacement draft must generate")
        .draft
        .expect("replacement draft must exist");
    let second_id = second.id;
    let expected_task_count = second.content.tasks.len();
    service
        .publish_confirmed(fixture.home.clone(), second_id, true)
        .expect("replacement plan must publish");

    let workspace = service
        .load_workspace(fixture.home.clone())
        .expect("workspace must load");
    assert_eq!(workspace.work_items.len(), expected_task_count);
    assert!(
        workspace
            .work_items
            .iter()
            .all(|view| view.work_item.plan_revision_id == second_id)
    );
    let inherited = workspace
        .work_items
        .iter()
        .find(|view| view.work_item.task_key == claimed.work_item.task_key)
        .expect("inherited task must exist");
    assert_eq!(inherited.work_item.status, WorkItemStatus::Done);

    let integrations = IntegrationService::with_dependencies(secrets, GitAdapter::new(), None);
    let release = integrations
        .load_workspace(fixture.home.clone())
        .expect("integration workspace must load")
        .release;
    for view in release
        .prerequisites
        .iter()
        .chain(release.milestones.iter())
        .chain(release.approval_gates.iter())
        .chain(release.human_tasks.iter())
    {
        assert_eq!(view.work_item.plan_revision_id, second_id);
    }

    let database = Database::open(&fixture.home.join(".coldstart").join("state.db"))
        .expect("database must reopen");
    assert!(
        database
            .list_work_items(&fixture.project_id)
            .expect("history must load")
            .len()
            > expected_task_count
    );
    assert_eq!(
        database
            .load_plan(&first.id)
            .expect("old plan must load")
            .expect("old plan must remain")
            .status,
        coldstart_core::PlanStatus::Superseded
    );
}

#[test]
fn task_context_and_references_use_only_the_active_plan() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let first = service
        .generate_draft(fixture.home.clone())
        .expect("first plan draft must generate")
        .draft
        .expect("first draft must exist");
    let first_workspace = service
        .publish(fixture.home.clone(), first.id)
        .expect("first plan must publish");
    let claimed = service
        .claim_next(
            fixture.home.clone(),
            ClaimRequest {
                agent_profile: "codex".to_owned(),
                repository: Some("web".to_owned()),
                request_id: "active-context-first".to_owned(),
            },
        )
        .expect("claim must call");
    let ClaimResult::Claimed(claimed) = claimed else {
        panic!("first work must be claimed");
    };
    let foundation_task = first_workspace
        .work_items
        .iter()
        .find(|view| view.work_item.id == claimed.work_item.id)
        .expect("claimed task must exist")
        .task
        .clone();
    service
        .submit_report(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token,
            report_for(&foundation_task, "active-context-foundation"),
        )
        .expect("foundation report must submit");

    let second = service
        .generate_draft(fixture.home.clone())
        .expect("replacement draft must generate")
        .draft
        .expect("replacement draft must exist");
    let second_id = second.id;
    service
        .publish_confirmed(fixture.home.clone(), second_id, true)
        .expect("replacement plan must publish");

    let connection = rusqlite::Connection::open(fixture.home.join(".coldstart").join("state.db"))
        .expect("tamper connection must open");
    connection
        .execute(
            "UPDATE work_items SET status = 'ready', priority = 999999 WHERE id = ?1",
            [claimed.work_item.id.to_string()],
        )
        .expect("historical work item must update");
    drop(connection);

    let workspace = service
        .load_workspace(fixture.home.clone())
        .expect("replacement workspace must load");
    let dependent = workspace
        .work_items
        .iter()
        .find(|view| {
            view.task.kind == coldstart_core::TaskKind::AgentCoding
                && view
                    .task
                    .dependencies
                    .iter()
                    .any(|dependency| dependency.task_key == foundation_task.task_key)
        })
        .expect("dependent task must exist");
    let dependent_key = dependent.work_item.task_key.clone();
    let claimed = service
        .claim_next(
            fixture.home.clone(),
            ClaimRequest {
                agent_profile: "codex".to_owned(),
                repository: Some("web".to_owned()),
                request_id: "active-context-dependent".to_owned(),
            },
        )
        .expect("dependent claim must call");
    let ClaimResult::Claimed(claimed) = claimed else {
        panic!("dependent work must be claimed");
    };
    assert_eq!(claimed.work_item.task_key, dependent_key);

    let context = service
        .get_context(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token.clone(),
        )
        .expect("context must compile");
    assert!(
        context
            .markdown
            .contains(&format!("- {}：Done（Blocks）", foundation_task.task_key))
    );
    assert!(
        !context
            .markdown
            .contains(&format!("- {}：Ready（Blocks）", foundation_task.task_key))
    );

    let reference = service
        .read_reference(
            fixture.home,
            claimed.work_item.id,
            claimed.lease_token,
            foundation_task.task_key,
        )
        .expect("dependency reference must load");
    assert!(reference.content.contains("状态：Done"));
    assert!(reference.content.contains(&second_id.to_string()));
    assert!(!reference.content.contains(&first.id.to_string()));
}

#[test]
fn failed_verification_keeps_the_lease_uses_the_frozen_snapshot_and_retry_completes() {
    let fixture = Fixture::new();
    let git = Arc::new(support::FakePlanGit::passing());
    let executor = Arc::new(support::ScriptedExecutor::new(vec![
        support::ScriptedExecutor::failure(
            9,
            "TOKEN=ghp_fakeSecretValue1234567890\nassertion failed",
        ),
    ]));
    let service = support::service(git.clone(), executor.clone());
    let claimed = publish_and_claim(&service, &fixture, "verification-retry-claim");
    let task = task_for(&service, &fixture.home, &claimed.work_item.task_key);

    let mut database = Database::open(&fixture.home.join(".coldstart").join("state.db"))
        .expect("database must open");
    database
        .replace_repository_verification_commands(
            &fixture.project_id,
            "web",
            vec![support::verification_command("changed-after-claim")],
        )
        .expect("desktop command update must save");
    drop(database);

    let context = service
        .get_context(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token.clone(),
        )
        .expect("context must compile");
    let snapshot = context
        .verification
        .as_ref()
        .expect("snapshot must be present");
    assert_eq!(snapshot.commands[0].name, "verify");
    assert_eq!(
        snapshot.content_hash(),
        claimed.run.verification_snapshot_hash.clone().unwrap()
    );
    assert!(context.markdown.contains("verify"));

    let error = service
        .submit_report(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token.clone(),
            report_for(&task, "verification-failed-report"),
        )
        .expect_err("failing command must reject the report");
    assert_eq!(error.code(), "verification_failed");
    let details = error
        .verification_details()
        .expect("failure details must exist");
    assert_eq!(details["command"], "verify");
    assert_eq!(details["exitCode"], 9);
    assert_eq!(details["failureKind"], "non_zero_exit");
    assert_eq!(details["durationMs"], 12);
    assert!(
        details["outputSummary"]
            .as_str()
            .unwrap()
            .contains("assertion failed")
    );
    assert!(
        !details
            .to_string()
            .contains("ghp_fakeSecretValue1234567890")
    );
    assert_eq!(executor.calls(), vec!["verify"]);
    assert_eq!(git.push_calls(), 0);

    let workspace = service
        .load_workspace(fixture.home.clone())
        .expect("workspace must load");
    let view = workspace
        .work_items
        .iter()
        .find(|view| view.work_item.id == claimed.work_item.id)
        .expect("work item must exist");
    assert_eq!(view.work_item.status, WorkItemStatus::InProgress);
    assert!(
        workspace
            .leases
            .iter()
            .any(|lease| lease.status == coldstart_core::LeaseStatus::Active)
    );
    let report = view
        .latest_report
        .as_ref()
        .expect("failed report must exist");
    assert!(!report.accepted);
    assert_eq!(report.verification_status, VerificationStatus::Failed);

    let success = service
        .submit_report(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token,
            report_for(&task, "verification-retry-success"),
        )
        .expect("retry after fixing the command must complete");
    assert!(success.accepted);
    assert_eq!(success.verification_status, VerificationStatus::Passed);
    assert_eq!(git.push_calls(), 1);
    assert_eq!(git.verify_calls(), 2);
    let workspace = service
        .load_workspace(fixture.home)
        .expect("final workspace must load");
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
}

#[test]
fn timeout_and_launch_failures_map_to_stable_errors_without_pushing() {
    for (execution, expected_code, expected_kind) in [
        (
            support::ScriptedExecutor::timeout(),
            "verification_timeout",
            "timeout",
        ),
        (
            support::ScriptedExecutor::launch_failure(),
            "verification_failed",
            "launch_failed",
        ),
    ] {
        let fixture = Fixture::new();
        let git = Arc::new(support::FakePlanGit::passing());
        let executor = Arc::new(support::ScriptedExecutor::new(vec![execution]));
        let service = support::service(git.clone(), executor);
        let claimed = publish_and_claim(&service, &fixture, "verification-error-claim");
        let task = task_for(&service, &fixture.home, &claimed.work_item.task_key);
        let error = service
            .submit_report(
                fixture.home.clone(),
                claimed.work_item.id,
                claimed.lease_token,
                report_for(&task, "verification-error-report"),
            )
            .expect_err("verification infrastructure failure must reject the report");
        assert_eq!(error.code(), expected_code);
        let details = error
            .verification_details()
            .expect("failure details must exist");
        assert_eq!(details["failureKind"], expected_kind);
        assert_eq!(git.push_calls(), 0);
    }
}

#[test]
fn git_invariant_damage_fails_verification_without_running_reported_commands() {
    let fixture = Fixture::new();
    let git = Arc::new(support::FakePlanGit::passing());
    git.fail_checkout(vec!["src/main.rs".to_owned()]);
    let executor = Arc::new(support::ScriptedExecutor::default());
    let service = support::service(git.clone(), executor.clone());
    let claimed = publish_and_claim(&service, &fixture, "git-invariant-claim");
    let task = task_for(&service, &fixture.home, &claimed.work_item.task_key);

    let error = service
        .submit_report(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token,
            report_for(&task, "git-invariant-report"),
        )
        .expect_err("Git invariant damage must reject the report");
    assert_eq!(error.code(), "verification_failed");
    let details = error
        .verification_details()
        .expect("failure details must exist");
    assert_eq!(details["failureKind"], "git_invariant");
    assert!(executor.calls().is_empty());
    assert_eq!(git.push_calls(), 0);
}

#[test]
fn a_claim_without_verification_commands_creates_no_lease_or_run() {
    let fixture = Fixture::new();
    let mut database = Database::open(&fixture.home.join(".coldstart").join("state.db"))
        .expect("database must open");
    database
        .replace_repository_verification_commands(&fixture.project_id, "web", vec![])
        .expect("empty verification set must be stored");
    drop(database);

    let service = fixture.service();
    let draft = service
        .generate_draft(fixture.home.clone())
        .expect("draft must generate")
        .draft
        .expect("draft must exist");
    service
        .publish(fixture.home.clone(), draft.id)
        .expect("plan must publish");
    let result = service
        .claim_next(
            fixture.home.clone(),
            ClaimRequest {
                agent_profile: "codex".to_owned(),
                repository: Some("web".to_owned()),
                request_id: "missing-verification-claim".to_owned(),
            },
        )
        .expect("claim must return a structured empty result");
    assert!(matches!(
        result,
        ClaimResult::Empty(ClaimFailure::VerificationNotConfigured)
    ));
    let workspace = service
        .load_workspace(fixture.home)
        .expect("workspace must load");
    assert!(workspace.leases.is_empty());
    assert!(workspace.runs.is_empty());
}

#[test]
fn push_failure_is_recorded_after_verification_without_reopening_the_task() {
    let fixture = Fixture::new();
    let git = Arc::new(support::FakePlanGit::passing());
    git.fail_push("remote rejected the branch");
    let executor = Arc::new(support::ScriptedExecutor::default());
    let service = support::service(git.clone(), executor);
    let claimed = publish_and_claim(&service, &fixture, "push-after-verification-claim");
    let task = task_for(&service, &fixture.home, &claimed.work_item.task_key);
    let report = service
        .submit_report(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token,
            report_for(&task, "push-after-verification-report"),
        )
        .expect("push failure must not undo a verified task");
    assert!(report.accepted);
    assert_eq!(git.push_calls(), 1);

    let workspace = service
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
    let integrations = IntegrationService::with_dependencies(
        Arc::new(MemorySecretStore::default()),
        GitAdapter::new(),
        None,
    )
    .load_workspace(fixture.home)
    .expect("integration workspace must load");
    assert!(integrations.git_syncs.iter().any(|sync| {
        sync.status == coldstart_core::GitSyncStatus::Failed
            && sync
                .error_message
                .as_deref()
                .is_some_and(|message| message.contains("remote rejected the branch"))
    }));
}
