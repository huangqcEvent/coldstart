use std::{fs, path::PathBuf, sync::Arc};

use coldstart_adapters::ReviewModelError;
use coldstart_app::{
    AppError, ClaimRequest, PlanService, ProjectService, ReviewService, ReviewStandardImport,
    ReviewStandardManualInput, ReviewStandardRevisionInput,
};
use coldstart_core::{
    AcceptanceResult, ArtifactFormat, ArtifactId, BlueprintContent, BlueprintEntityId,
    BlueprintGeneration, ClaimResult, ContextReferenceKind, GenerationSource, ModuleDefinition,
    PlatformDefinition, PlatformKind, ProjectId, ReportOutcome, ReviewCategory,
    ReviewConfigurationInput, ReviewFinding, ReviewScope, ReviewSeverity, ReviewStatus,
    SourceReference, TraceableText, WorkItemStatus, WorkReportInput,
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
    fn new(configure_repository: bool) -> Self {
        let directory = tempfile::tempdir().expect("temporary directory");
        let home = directory.path().join("project");
        let status = ProjectService::new()
            .create_project("Review 测试项目", home.clone())
            .expect("project");
        let mut database =
            Database::open(&home.join(".coldstart").join("state.db")).expect("database");
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
            .expect("blueprint revision");
        database
            .publish_blueprint_revision(&status.project_id, &revision.id)
            .expect("publish blueprint");
        if configure_repository {
            support::configure_repository(&home, status.project_id, "web", true);
        }
        Self {
            _directory: directory,
            home,
            project_id: status.project_id,
        }
    }

    fn service(&self, review_model: Arc<support::ScriptedReviewModel>) -> PlanService {
        support::service_with_review_model(
            Arc::new(coldstart_adapters::MemorySecretStore::default()),
            Arc::new(support::FakePlanGit::passing()),
            Arc::new(support::ScriptedExecutor::default()),
            review_model,
        )
    }

    fn configure_review(&self) {
        ReviewService::new()
            .save_configuration(
                self.home.clone(),
                ReviewConfigurationInput {
                    enabled: true,
                    base_url: "https://review.example/v1".to_owned(),
                    model: "reviewer".to_owned(),
                    credential_id: None,
                    blocking_severities: vec![ReviewSeverity::Critical, ReviewSeverity::High],
                    consent_confirmed: true,
                },
            )
            .expect("review configuration");
        ReviewService::new()
            .create_manual_standard(
                self.home.clone(),
                ReviewStandardManualInput {
                    name: "Web 规则".to_owned(),
                    scope: ReviewScope::Repository("web".to_owned()),
                    text: "禁止使用 innerHTML。".to_owned(),
                },
            )
            .expect("review standard");
    }

    fn publish_and_claim(&self, service: &PlanService) -> coldstart_core::ClaimedWork {
        let draft = service
            .generate_draft(self.home.clone())
            .expect("draft")
            .draft
            .expect("draft exists");
        service
            .publish(self.home.clone(), draft.id)
            .expect("publish plan");
        match service
            .claim_next(
                self.home.clone(),
                ClaimRequest {
                    agent_profile: "codex".to_owned(),
                    repository: Some("web".to_owned()),
                    request_id: "review-claim".to_owned(),
                },
            )
            .expect("claim")
        {
            ClaimResult::Claimed(claimed) => claimed,
            other => panic!("expected claimed work, got {other:?}"),
        }
    }

    fn task(
        &self,
        service: &PlanService,
        claimed: &coldstart_core::ClaimedWork,
    ) -> coldstart_core::TaskSpec {
        service
            .load_workspace(self.home.clone())
            .expect("workspace")
            .work_items
            .into_iter()
            .find(|view| view.work_item.id == claimed.work_item.id)
            .expect("work item")
            .task
    }

    fn report(&self, task: &coldstart_core::TaskSpec, key: &str) -> WorkReportInput {
        WorkReportInput {
            status: ReportOutcome::Completed,
            summary: "完成 Review 测试任务".to_owned(),
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
                    evidence_refs: vec!["test".to_owned()],
                })
                .collect(),
            commands_run: vec![],
            evidence_refs: vec!["test".to_owned()],
            known_issues: vec![],
            follow_up_items: vec![],
            contract_hash: task.contract_hash(),
            idempotency_key: key.to_owned(),
        }
    }

    fn latest_report(&self) -> coldstart_core::WorkReportRecord {
        Database::open(&self.home.join(".coldstart").join("state.db"))
            .expect("database")
            .list_work_reports(&self.project_id)
            .expect("reports")
            .into_iter()
            .max_by_key(|report| report.created_at)
            .expect("report")
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

fn blocking_finding() -> ReviewFinding {
    ReviewFinding {
        severity: ReviewSeverity::High,
        category: ReviewCategory::Security,
        file: "src/main.rs".to_owned(),
        line: Some(1),
        message: "发现不安全写入".to_owned(),
        recommendation: "使用安全 API".to_owned(),
        standard: Some("Web 规则".to_owned()),
    }
}

#[test]
fn imports_versions_and_validates_review_standards() {
    let fixture = Fixture::new(false);
    let source = fixture.home.join("security.md");
    fs::write(&source, "# 安全\n\n禁止记录令牌。\n").expect("source file");
    let imported = ReviewService::new()
        .import_standard(
            fixture.home.clone(),
            ReviewStandardImport {
                name: "安全基线".to_owned(),
                scope: ReviewScope::Project,
                path: source,
            },
        )
        .expect("import standard");
    assert_eq!(imported.format, ArtifactFormat::Markdown);
    assert!(
        fixture
            .home
            .join(".coldstart")
            .join("review-standards")
            .join(imported.id.to_string())
            .join(imported.current_revision_id.to_string())
            .join("extracted.txt")
            .exists()
    );
    let read = ReviewService::new()
        .read_standard(fixture.home.clone(), imported.id)
        .expect("read standard");
    assert!(read.text.contains("禁止记录令牌"));

    let oversized = "x".repeat(60_001);
    let error = ReviewService::new()
        .create_manual_standard(
            fixture.home.clone(),
            ReviewStandardManualInput {
                name: "超大规范".to_owned(),
                scope: ReviewScope::Project,
                text: oversized,
            },
        )
        .expect_err("oversized standard");
    assert_eq!(error.code(), "review_input_too_large");

    let revised = ReviewService::new()
        .revise_standard(
            fixture.home.clone(),
            imported.id,
            ReviewStandardRevisionInput {
                name: "安全基线".to_owned(),
                scope: ReviewScope::Project,
                text: "禁止输出密码。".to_owned(),
            },
        )
        .expect("revise");
    assert_eq!(revised.revision_number, 2);
    assert_ne!(revised.current_revision_id, imported.current_revision_id);

    let path = fixture
        .home
        .join(".coldstart")
        .join("review-standards")
        .join(revised.id.to_string())
        .join(revised.current_revision_id.to_string())
        .join("extracted.txt");
    fs::write(path, "tampered").expect("tamper");
    let error = ReviewService::new()
        .read_standard(fixture.home.clone(), revised.id)
        .expect_err("hash mismatch");
    assert_eq!(error.code(), "review_standard_invalid");
}

#[test]
fn review_configuration_requires_new_consent_after_model_change() {
    let fixture = Fixture::new(false);
    let service = ReviewService::new();
    let valid = ReviewConfigurationInput {
        enabled: true,
        base_url: "https://review.example/v1".to_owned(),
        model: "reviewer".to_owned(),
        credential_id: None,
        blocking_severities: vec![ReviewSeverity::Critical, ReviewSeverity::High],
        consent_confirmed: true,
    };
    service
        .save_configuration(fixture.home.clone(), valid.clone())
        .expect("save");
    let mut changed = valid;
    changed.model = "other".to_owned();
    changed.consent_confirmed = false;
    assert!(matches!(
        service.save_configuration(fixture.home.clone(), changed),
        Err(AppError::ReviewConsentRequired)
    ));
}

#[test]
fn claim_freezes_standards_and_context_reads_the_frozen_revision() {
    let fixture = Fixture::new(true);
    fixture.configure_review();
    let service = fixture.service(Arc::new(support::ScriptedReviewModel::new(vec![])));
    let claimed = fixture.publish_and_claim(&service);
    let snapshot = claimed
        .run
        .review_snapshot
        .as_ref()
        .expect("review snapshot");
    assert_eq!(snapshot.repository_key, "web");
    assert_eq!(snapshot.standards.len(), 1);

    let context = service
        .get_context(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token.clone(),
        )
        .expect("context");
    let reference = context
        .included
        .iter()
        .find(|reference| reference.kind == ContextReferenceKind::ReviewStandard)
        .expect("review reference");
    let before = service
        .read_reference(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token.clone(),
            reference.id.clone(),
        )
        .expect("read reference");
    assert!(before.content.contains("innerHTML"));

    let standard = ReviewService::new()
        .load_workspace(fixture.home.clone())
        .expect("workspace")
        .standards
        .into_iter()
        .next()
        .expect("standard");
    ReviewService::new()
        .revise_standard(
            fixture.home.clone(),
            standard.id,
            ReviewStandardRevisionInput {
                name: standard.name,
                scope: ReviewScope::Repository("web".to_owned()),
                text: "新规范正文".to_owned(),
            },
        )
        .expect("revise after claim");
    let after = service
        .read_reference(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token,
            reference.id.clone(),
        )
        .expect("old snapshot reference");
    assert_eq!(after.content, before.content);
    assert_eq!(after.content_hash, before.content_hash);
}

#[test]
fn blocked_review_does_not_push_and_can_be_waived_once() {
    let fixture = Fixture::new(true);
    fixture.configure_review();
    let model = Arc::new(support::ScriptedReviewModel::new(vec![Ok(
        support::review_output(vec![blocking_finding()]),
    )]));
    let service = fixture.service(model.clone());
    let claimed = fixture.publish_and_claim(&service);
    let task = fixture.task(&service, &claimed);
    let error = service
        .submit_report(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token.clone(),
            fixture.report(&task, "attempt-1"),
        )
        .expect_err("review must block");
    assert_eq!(error.code(), "review_blocked");
    assert_eq!(model.calls(), 1);
    let report = fixture.latest_report();
    assert_eq!(report.review_status, ReviewStatus::Blocked);
    assert!(!report.accepted);
    let database =
        Database::open(&fixture.home.join(".coldstart").join("state.db")).expect("database");
    assert_eq!(
        database
            .load_work_item(&claimed.work_item.id)
            .expect("work item")
            .expect("exists")
            .status,
        WorkItemStatus::InProgress
    );
    assert!(
        database
            .list_git_syncs(&fixture.project_id)
            .expect("syncs")
            .is_empty()
    );

    let waived = service
        .waive_review(
            fixture.home.clone(),
            claimed.work_item.id,
            report.id,
            "已人工核对为误报".to_owned(),
        )
        .expect("waive");
    assert_eq!(waived.review_status, ReviewStatus::Waived);
    assert!(waived.accepted);
    let database =
        Database::open(&fixture.home.join(".coldstart").join("state.db")).expect("database");
    assert_eq!(
        database
            .list_git_syncs(&fixture.project_id)
            .expect("syncs")
            .iter()
            .filter(|sync| sync.status == coldstart_core::GitSyncStatus::Pushed)
            .count(),
        1
    );
}

#[test]
fn model_timeout_fails_closed_and_keeps_the_lease() {
    let fixture = Fixture::new(true);
    fixture.configure_review();
    let model = Arc::new(support::ScriptedReviewModel::new(vec![Err(
        ReviewModelError::Timeout,
    )]));
    let service = fixture.service(model);
    let claimed = fixture.publish_and_claim(&service);
    let task = fixture.task(&service, &claimed);
    let error = service
        .submit_report(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token,
            fixture.report(&task, "timeout-1"),
        )
        .expect_err("timeout");
    assert_eq!(error.code(), "review_timeout");
    let report = fixture.latest_report();
    assert_eq!(report.review_status, ReviewStatus::Failed);
    assert!(!report.accepted);
    let database =
        Database::open(&fixture.home.join(".coldstart").join("state.db")).expect("database");
    assert!(
        database
            .list_git_syncs(&fixture.project_id)
            .expect("syncs")
            .is_empty()
    );
    assert_eq!(
        database
            .load_work_item(&claimed.work_item.id)
            .expect("work item")
            .expect("exists")
            .status,
        WorkItemStatus::InProgress
    );
}

#[test]
fn passing_review_accepts_and_pushes_once() {
    let fixture = Fixture::new(true);
    fixture.configure_review();
    let model = Arc::new(support::ScriptedReviewModel::new(vec![Ok(
        support::review_output(vec![]),
    )]));
    let service = fixture.service(model);
    let claimed = fixture.publish_and_claim(&service);
    let task = fixture.task(&service, &claimed);
    let record = service
        .submit_report(
            fixture.home.clone(),
            claimed.work_item.id,
            claimed.lease_token,
            fixture.report(&task, "pass-1"),
        )
        .expect("review passes");
    assert_eq!(record.review_status, ReviewStatus::Passed);
    assert!(record.accepted);
    let database =
        Database::open(&fixture.home.join(".coldstart").join("state.db")).expect("database");
    assert_eq!(
        database
            .list_git_syncs(&fixture.project_id)
            .expect("syncs")
            .iter()
            .filter(|sync| sync.status == coldstart_core::GitSyncStatus::Pushed)
            .count(),
        1
    );
}
