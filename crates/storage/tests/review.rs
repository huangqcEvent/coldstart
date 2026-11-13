mod support;

use chrono::{Duration, Utc};
use coldstart_core::{
    ArtifactFormat, ClaimResult, ReviewCategory, ReviewConfiguration, ReviewConfigurationInput,
    ReviewFailure, ReviewFinding, ReviewResult, ReviewScope, ReviewSeverity, ReviewStandardId,
    ReviewStandardRevisionId, ReviewStandardSource, ReviewStatus, VerificationCommandResult,
    VerificationResult, VerificationStatus, WorkItemStatus, sha256_hex,
};
use coldstart_storage::{BeginReview, BeginVerification, NewReviewStandardRevision};
use support::{PlanFixture, agent_task, report_for};

const BASE_COMMIT: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn review_configuration(project_id: coldstart_core::ProjectId) -> ReviewConfiguration {
    ReviewConfiguration::from_input(
        project_id,
        ReviewConfigurationInput {
            enabled: true,
            base_url: "https://review.example.invalid/v1".to_owned(),
            model: "review-model".to_owned(),
            credential_id: None,
            blocking_severities: vec![ReviewSeverity::Critical, ReviewSeverity::High],
            consent_confirmed: true,
        },
    )
    .expect("review configuration must be valid")
}

fn add_standard(
    fixture: &mut PlanFixture,
    standard_id: ReviewStandardId,
    name: &str,
    scope: ReviewScope,
    text: &str,
) -> coldstart_core::ReviewStandard {
    let revision_id = ReviewStandardRevisionId::new();
    fixture
        .database
        .create_review_standard_revision(
            &fixture.project_id,
            NewReviewStandardRevision {
                standard_id,
                revision_id,
                name: name.to_owned(),
                scope,
                source: ReviewStandardSource::Manual,
                file_name: "standard.md".to_owned(),
                format: ArtifactFormat::Markdown,
                content_hash: sha256_hex(text.as_bytes()),
                text_hash: sha256_hex(text.as_bytes()),
                character_count: text.chars().count(),
                storage_relative_path: format!(
                    "review-standards/{standard_id}/{revision_id}/extracted.txt"
                ),
            },
            Utc::now(),
        )
        .expect("review standard must save")
}

fn claim_with_review(
    fixture: &mut PlanFixture,
    request_id: &str,
    now: chrono::DateTime<Utc>,
) -> coldstart_core::ClaimedWork {
    let result = fixture
        .database
        .claim_work_with_review(
            &fixture.project_id,
            "codex",
            Some("server"),
            request_id,
            now,
            Duration::minutes(30),
            Some(BASE_COMMIT),
        )
        .expect("review claim must run");
    let ClaimResult::Claimed(claimed) = result else {
        panic!("work must be claimed: {result:?}");
    };
    claimed
}

fn passed_verification(snapshot_hash: String, now: chrono::DateTime<Utc>) -> VerificationResult {
    VerificationResult {
        snapshot_hash,
        status: VerificationStatus::Passed,
        started_at: now,
        finished_at: Some(now + Duration::seconds(1)),
        total_duration_ms: Some(1_000),
        failure_kind: None,
        failure_command: None,
        commands: vec![VerificationCommandResult {
            name: "test".to_owned(),
            exit_code: Some(0),
            duration_ms: 1_000,
            output_summary: "ok".to_owned(),
            truncated: false,
            failure_kind: None,
        }],
    }
}

fn begin_review(fixture: &mut PlanFixture) -> coldstart_storage::PendingReview {
    fixture
        .database
        .save_review_configuration(&review_configuration(fixture.project_id))
        .expect("review configuration must save");
    add_standard(
        fixture,
        ReviewStandardId::new(),
        "通用安全基线",
        ReviewScope::Project,
        "禁止记录令牌",
    );
    let claim_time = Utc::now();
    let claimed = claim_with_review(fixture, "review-claim", claim_time);
    let task = fixture.task("foundation:demo");
    let begin_time = claim_time + Duration::seconds(1);
    let BeginVerification::Started(verification) = fixture
        .database
        .begin_work_verification(
            &claimed.work_item.id,
            &claimed.lease_token,
            report_for(&task, "review-report"),
            begin_time,
            Duration::minutes(30),
        )
        .expect("verification must begin")
    else {
        panic!("new report must start verification");
    };
    let review = fixture
        .database
        .finish_work_verification_and_begin_review(
            &fixture.project_id,
            &claimed.work_item.id,
            &verification.report.id,
            passed_verification(
                verification.snapshot.content_hash(),
                begin_time + Duration::seconds(1),
            ),
            begin_time + Duration::seconds(2),
        )
        .expect("verification must transition to review");
    let BeginReview::Started(pending) = review else {
        panic!("review must start: {review:?}");
    };
    pending
}

fn result_for(
    pending: &coldstart_storage::PendingReview,
    status: ReviewStatus,
    findings: Vec<ReviewFinding>,
    now: chrono::DateTime<Utc>,
) -> ReviewResult {
    ReviewResult {
        snapshot_hash: pending.snapshot.content_hash(),
        status,
        summary: "评审完成".to_owned(),
        findings,
        model: "review-model".to_owned(),
        failure: (status == ReviewStatus::Failed).then(|| ReviewFailure {
            code: "review_failed".to_owned(),
            message: "模型不可用".to_owned(),
        }),
        started_at: now,
        finished_at: Some(now + Duration::seconds(3)),
        duration_ms: Some(3_000),
    }
}

#[test]
fn versions_project_and_repository_standards_for_the_next_claim() {
    let mut fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    assert_eq!(fixture.database.schema_version().expect("schema"), 9);
    let common_id = ReviewStandardId::new();
    let common = add_standard(
        &mut fixture,
        common_id,
        "通用安全基线",
        ReviewScope::Project,
        "禁止记录令牌",
    );
    let web = add_standard(
        &mut fixture,
        ReviewStandardId::new(),
        "Web 规则",
        ReviewScope::Repository("web".to_owned()),
        "禁止 innerHTML",
    );
    let first_revision = common.current_revision.id;
    let revised = add_standard(
        &mut fixture,
        common_id,
        "通用安全基线",
        ReviewScope::Project,
        "禁止记录任何令牌",
    );
    assert_ne!(first_revision, revised.current_revision.id);
    assert_eq!(revised.current_revision.revision_number, 2);

    let applicable = fixture
        .database
        .applicable_review_standards(&fixture.project_id, "web")
        .expect("applicable standards");
    assert_eq!(
        applicable
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>(),
        vec!["通用安全基线", "Web 规则"]
    );
    assert_eq!(applicable[1].id, web.id);
}

#[test]
fn claim_freezes_review_configuration_standards_and_base_commit() {
    let mut fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    fixture
        .database
        .save_review_configuration(&review_configuration(fixture.project_id))
        .expect("configuration must save");
    let standard = add_standard(
        &mut fixture,
        ReviewStandardId::new(),
        "通用安全基线",
        ReviewScope::Project,
        "禁止记录令牌",
    );
    let claimed = claim_with_review(&mut fixture, "frozen-claim", Utc::now());
    let snapshot = claimed
        .run
        .review_snapshot
        .clone()
        .expect("review snapshot");
    assert_eq!(snapshot.base_commit, BASE_COMMIT);
    assert_eq!(snapshot.standards.len(), 1);
    assert_eq!(
        snapshot.standards[0].revision_id,
        standard.current_revision.id
    );
    assert_eq!(
        claimed.run.review_snapshot_hash.as_deref(),
        Some(snapshot.content_hash().as_str())
    );

    add_standard(
        &mut fixture,
        standard.id,
        "通用安全基线",
        ReviewScope::Project,
        "规则已经更新",
    );
    let stored = fixture
        .database
        .list_work_runs(&fixture.project_id)
        .expect("runs")
        .into_iter()
        .find(|run| run.id == claimed.run.id)
        .expect("run must exist");
    assert_eq!(stored.review_snapshot.expect("stored snapshot"), snapshot);
}

#[test]
fn blocked_review_keeps_task_in_progress_and_waiver_completes_it() {
    let mut fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    let pending = begin_review(&mut fixture);
    let now = Utc::now();
    let blocked = fixture
        .database
        .finish_work_review_failure(
            &fixture.project_id,
            &pending.report.work_item_id,
            &pending.report.id,
            result_for(
                &pending,
                ReviewStatus::Blocked,
                vec![ReviewFinding {
                    severity: ReviewSeverity::High,
                    category: ReviewCategory::Security,
                    file: "src/main.rs".to_owned(),
                    line: Some(12),
                    message: "记录了令牌".to_owned(),
                    recommendation: "移除令牌日志".to_owned(),
                    standard: Some("通用安全基线".to_owned()),
                }],
                now,
            ),
            now,
            Duration::minutes(30),
        )
        .expect("blocked review must persist");
    assert_eq!(blocked.review_status, ReviewStatus::Blocked);
    assert!(!blocked.accepted);
    let work = fixture
        .database
        .load_work_item(&pending.report.work_item_id)
        .expect("work item")
        .expect("work item exists");
    assert_eq!(work.status, WorkItemStatus::InProgress);
    assert_eq!(
        fixture
            .database
            .list_leases(&fixture.project_id)
            .expect("leases")[0]
            .status,
        coldstart_core::LeaseStatus::Active
    );

    let waived = fixture
        .database
        .waive_work_review(
            &fixture.project_id,
            &pending.report.work_item_id,
            &pending.report.id,
            "确认是误报",
            "local_desktop",
            now + Duration::seconds(2),
        )
        .expect("waiver must complete the task");
    assert_eq!(waived.review_status, ReviewStatus::Waived);
    assert!(waived.accepted);
    assert_eq!(
        waived.review_waiver.as_ref().expect("waiver").reason,
        "确认是误报"
    );
    assert_eq!(
        fixture
            .database
            .load_work_item(&pending.report.work_item_id)
            .expect("work item")
            .expect("work item exists")
            .status,
        WorkItemStatus::Done
    );
}

#[test]
fn passed_review_completes_work_and_duplicate_finish_is_idempotent() {
    let mut fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    let pending = begin_review(&mut fixture);
    let now = Utc::now();
    let review = result_for(&pending, ReviewStatus::Passed, Vec::new(), now);
    let passed = fixture
        .database
        .finish_work_review_success(
            &fixture.project_id,
            &pending.report.work_item_id,
            &pending.report.id,
            review.clone(),
            now,
        )
        .expect("passing review must complete work");
    assert_eq!(passed.review_status, ReviewStatus::Passed);
    assert!(passed.accepted);
    let again = fixture
        .database
        .finish_work_review_success(
            &fixture.project_id,
            &pending.report.work_item_id,
            &pending.report.id,
            review,
            now,
        )
        .expect("duplicate finish must be idempotent");
    assert_eq!(again.id, passed.id);
    assert_eq!(
        fixture
            .database
            .load_work_item(&pending.report.work_item_id)
            .expect("work item")
            .expect("work item exists")
            .status,
        WorkItemStatus::Done
    );
}

#[test]
fn expired_lease_interrupts_a_running_review() {
    let mut fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    let pending = begin_review(&mut fixture);
    fixture
        .database
        .expire_leases(&fixture.project_id, Utc::now() + Duration::minutes(31))
        .expect("lease expiry must run");
    let reports = fixture
        .database
        .list_work_reports(&fixture.project_id)
        .expect("reports");
    assert_eq!(reports[0].review_status, ReviewStatus::Interrupted);
    assert_eq!(
        fixture
            .database
            .load_work_item(&pending.report.work_item_id)
            .expect("work item")
            .expect("work item exists")
            .status,
        WorkItemStatus::Ready
    );
}
