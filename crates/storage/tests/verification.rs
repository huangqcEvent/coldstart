mod support;

use chrono::{Duration, Utc};
use coldstart_core::{
    ClaimFailure, ClaimResult, RepositoryVerificationCommand, VerificationCommandResult,
    VerificationFailureKind, VerificationResult, VerificationStatus, WorkItemStatus,
};
use coldstart_storage::BeginVerification;
use support::{PlanFixture, agent_task, report_for};

fn command(name: &str, position: u32) -> RepositoryVerificationCommand {
    RepositoryVerificationCommand {
        name: name.to_owned(),
        executable: std::env::current_exe().expect("test executable must exist"),
        args: vec!["--test".to_owned()],
        timeout_seconds: 600,
        position,
    }
}

fn claimed(
    fixture: &mut PlanFixture,
    request_id: &str,
    now: chrono::DateTime<Utc>,
) -> coldstart_core::ClaimedWork {
    let result = fixture
        .database
        .claim_work(
            &fixture.project_id,
            "codex",
            Some("server"),
            request_id,
            now,
            Duration::minutes(30),
        )
        .expect("claim must run");
    let ClaimResult::Claimed(claimed) = result else {
        panic!("work must be claimed: {result:?}");
    };
    claimed
}

fn passed(
    pending: &coldstart_storage::PendingVerification,
    now: chrono::DateTime<Utc>,
) -> VerificationResult {
    VerificationResult {
        snapshot_hash: pending.snapshot.content_hash(),
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

fn failed(
    pending: &coldstart_storage::PendingVerification,
    now: chrono::DateTime<Utc>,
) -> VerificationResult {
    VerificationResult {
        snapshot_hash: pending.snapshot.content_hash(),
        status: VerificationStatus::Failed,
        started_at: now,
        finished_at: Some(now + Duration::seconds(2)),
        total_duration_ms: Some(2_000),
        failure_kind: Some(VerificationFailureKind::NonZeroExit),
        failure_command: Some("test".to_owned()),
        commands: vec![VerificationCommandResult {
            name: "test".to_owned(),
            exit_code: Some(1),
            duration_ms: 2_000,
            output_summary: "failed".to_owned(),
            truncated: false,
            failure_kind: Some(VerificationFailureKind::NonZeroExit),
        }],
    }
}

#[test]
fn migrates_to_v8_and_replaces_ordered_commands() {
    let mut fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    assert_eq!(fixture.database.schema_version().expect("version"), 11);
    let saved = fixture
        .database
        .replace_repository_verification_commands(
            &fixture.project_id,
            "server",
            vec![command("build", 1), command("test", 0)],
        )
        .expect("commands must save");
    assert_eq!(
        saved
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>(),
        ["test", "build"]
    );
    assert_eq!(
        fixture
            .database
            .list_repository_verification_commands(&fixture.project_id, "server")
            .expect("commands must load"),
        saved
    );
}

#[test]
fn claim_without_verification_commands_creates_no_lease_or_run() {
    let mut fixture = PlanFixture::without_verification(vec![agent_task("foundation:demo", &[])]);
    let result = fixture
        .database
        .claim_work(
            &fixture.project_id,
            "codex",
            Some("server"),
            "no-verification",
            Utc::now(),
            Duration::minutes(30),
        )
        .expect("claim must run");
    assert_eq!(
        result,
        ClaimResult::Empty(ClaimFailure::VerificationNotConfigured)
    );
    assert!(
        fixture
            .database
            .list_work_runs(&fixture.project_id)
            .expect("runs")
            .is_empty()
    );
    assert!(
        fixture
            .database
            .list_leases(&fixture.project_id)
            .expect("leases")
            .is_empty()
    );
    assert_eq!(
        fixture
            .database
            .list_work_items(&fixture.project_id)
            .expect("items")[0]
            .status,
        WorkItemStatus::Ready
    );
}

#[test]
fn claim_freezes_the_verification_snapshot() {
    let mut fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    let original = fixture
        .database
        .list_repository_verification_commands(&fixture.project_id, "server")
        .expect("commands");
    let now = Utc::now();
    let claimed_work = claimed(&mut fixture, "snapshot-claim", now);
    let run = claimed_work.run;
    assert_eq!(
        run.verification_snapshot
            .as_ref()
            .expect("snapshot")
            .commands,
        original
    );
    assert_eq!(
        run.verification_snapshot_hash.as_deref(),
        Some(
            run.verification_snapshot
                .as_ref()
                .expect("snapshot")
                .content_hash()
                .as_str()
        )
    );

    fixture
        .database
        .replace_repository_verification_commands(
            &fixture.project_id,
            "server",
            vec![RepositoryVerificationCommand {
                timeout_seconds: 700,
                ..command("changed", 0)
            }],
        )
        .expect("configuration must change");
    let stored = fixture
        .database
        .list_work_runs(&fixture.project_id)
        .expect("runs")
        .into_iter()
        .find(|item| item.id == run.id)
        .expect("run must exist");
    assert_eq!(
        stored.verification_snapshot_hash,
        run.verification_snapshot_hash
    );
    assert_ne!(
        stored.verification_snapshot.expect("snapshot").commands[0].name,
        "changed"
    );
}

#[test]
fn successful_verification_consumes_the_lease_and_completes_work() {
    let mut fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    let claim_time = Utc::now();
    let claimed_work = claimed(&mut fixture, "successful-verification", claim_time);
    let task = fixture.task("foundation:demo");
    let report = report_for(&task, "successful-report");
    let begin_time = claim_time + Duration::seconds(1);
    let BeginVerification::Started(pending) = fixture
        .database
        .begin_work_verification(
            &claimed_work.work_item.id,
            &claimed_work.lease_token,
            report,
            begin_time,
            Duration::minutes(30),
        )
        .expect("verification must begin")
    else {
        panic!("new report must start verification");
    };
    assert_eq!(
        fixture
            .database
            .load_work_item(&claimed_work.work_item.id)
            .expect("item")
            .expect("item exists")
            .status,
        WorkItemStatus::Submitted
    );
    let lease = fixture
        .database
        .list_leases(&fixture.project_id)
        .expect("leases")[0]
        .clone();
    assert_eq!(lease.expires_at, begin_time + Duration::seconds(600 + 300));

    let record = fixture
        .database
        .finish_work_verification_success(
            &fixture.project_id,
            &claimed_work.work_item.id,
            &pending.report.id,
            passed(&pending, begin_time + Duration::seconds(2)),
            begin_time + Duration::seconds(3),
        )
        .expect("verification must complete");
    assert!(record.accepted);
    assert_eq!(record.verification_status, VerificationStatus::Passed);
    assert_eq!(
        fixture
            .database
            .load_work_item(&claimed_work.work_item.id)
            .expect("item")
            .expect("item exists")
            .status,
        WorkItemStatus::Done
    );
    assert_eq!(
        fixture
            .database
            .list_leases(&fixture.project_id)
            .expect("leases")[0]
            .status,
        coldstart_core::LeaseStatus::Consumed
    );
}

#[test]
fn failed_verification_keeps_the_lease_and_allows_a_new_report() {
    let mut fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    let claim_time = Utc::now();
    let claimed_work = claimed(&mut fixture, "retry-verification", claim_time);
    let task = fixture.task("foundation:demo");
    let first_time = claim_time + Duration::seconds(1);
    let BeginVerification::Started(first) = fixture
        .database
        .begin_work_verification(
            &claimed_work.work_item.id,
            &claimed_work.lease_token,
            report_for(&task, "failed-report"),
            first_time,
            Duration::minutes(30),
        )
        .expect("first verification must begin")
    else {
        panic!("first report must start");
    };
    assert!(matches!(
        fixture.database.begin_work_verification(
            &claimed_work.work_item.id,
            &claimed_work.lease_token,
            report_for(&task, "failed-report"),
            first_time + Duration::seconds(1),
            Duration::minutes(30),
        ),
        Err(coldstart_storage::StorageError::VerificationInProgress(_))
    ));
    let failed_record = fixture
        .database
        .finish_work_verification_failure(
            &fixture.project_id,
            &claimed_work.work_item.id,
            &first.report.id,
            failed(&first, first_time + Duration::seconds(2)),
            first_time + Duration::seconds(3),
            Duration::minutes(30),
        )
        .expect("verification failure must persist");
    assert!(!failed_record.accepted);
    assert_eq!(
        failed_record.verification_status,
        VerificationStatus::Failed
    );
    assert_eq!(
        fixture
            .database
            .load_work_item(&claimed_work.work_item.id)
            .expect("item")
            .expect("item exists")
            .status,
        WorkItemStatus::InProgress
    );
    assert_eq!(
        fixture
            .database
            .list_leases(&fixture.project_id)
            .expect("leases")[0]
            .status,
        coldstart_core::LeaseStatus::Active
    );
    let BeginVerification::AlreadyResolved(replayed) = fixture
        .database
        .begin_work_verification(
            &claimed_work.work_item.id,
            &claimed_work.lease_token,
            report_for(&task, "failed-report"),
            first_time + Duration::seconds(4),
            Duration::minutes(30),
        )
        .expect("same key must replay")
    else {
        panic!("failed report must not run twice");
    };
    assert_eq!(replayed.id, failed_record.id);

    let retry_time = first_time + Duration::seconds(5);
    let BeginVerification::Started(retry) = fixture
        .database
        .begin_work_verification(
            &claimed_work.work_item.id,
            &claimed_work.lease_token,
            report_for(&task, "fixed-report"),
            retry_time,
            Duration::minutes(30),
        )
        .expect("new key must retry")
    else {
        panic!("new report must start verification");
    };
    assert_eq!(retry.report.report.idempotency_key, "fixed-report");
}

#[test]
fn expired_lease_interrupts_a_running_verification() {
    let mut fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    let claim_time = Utc::now();
    let claimed_work = claimed(&mut fixture, "interrupted-verification", claim_time);
    let task = fixture.task("foundation:demo");
    let begin_time = claim_time + Duration::seconds(1);
    let BeginVerification::Started(pending) = fixture
        .database
        .begin_work_verification(
            &claimed_work.work_item.id,
            &claimed_work.lease_token,
            report_for(&task, "interrupted-report"),
            begin_time,
            Duration::minutes(30),
        )
        .expect("verification must begin")
    else {
        panic!("report must start verification");
    };
    let expired_at = begin_time + Duration::hours(2);
    assert_eq!(
        fixture
            .database
            .expire_leases(&fixture.project_id, expired_at)
            .expect("leases must expire"),
        1
    );
    let report = fixture
        .database
        .list_work_reports(&fixture.project_id)
        .expect("reports")
        .into_iter()
        .find(|item| item.id == pending.report.id)
        .expect("report must remain");
    assert_eq!(report.verification_status, VerificationStatus::Interrupted);
    assert!(!report.accepted);
    assert_eq!(
        report
            .verification
            .expect("verification result")
            .failure_kind,
        Some(VerificationFailureKind::Interrupted)
    );
    assert_eq!(
        fixture
            .database
            .load_work_item(&claimed_work.work_item.id)
            .expect("item")
            .expect("item exists")
            .status,
        WorkItemStatus::Ready
    );
}

#[test]
fn command_validation_rejects_secret_pipe_and_shell_configuration() {
    let mut fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    let error = fixture
        .database
        .replace_repository_verification_commands(
            &fixture.project_id,
            "server",
            vec![RepositoryVerificationCommand {
                executable: "cmd.exe".into(),
                ..command("shell", 0)
            }],
        )
        .expect_err("relative executable must be rejected");
    assert!(matches!(
        error,
        coldstart_storage::StorageError::VerificationCommandInvalid(_)
    ));
}
