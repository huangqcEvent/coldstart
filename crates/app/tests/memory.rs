use std::path::PathBuf;

use chrono::Utc;
use coldstart_app::{MemoryPhase, ProjectMemoryService, ProjectService};
use coldstart_core::{
    ArtifactId, BlueprintRevisionId, CredentialId, DomainEvent, DomainEventKind, GenerationSource,
    GitSyncId, InfoRequestId, LeaseId, PlanRevisionId, ProjectFactId, ProjectId, RepositoryId,
    WorkItemId, WorkReportId,
};
use coldstart_storage::Database;
use tempfile::TempDir;

struct MemoryFixture {
    _directory: TempDir,
    home: PathBuf,
    project_id: ProjectId,
}

fn fixture() -> MemoryFixture {
    let directory = TempDir::new().expect("temporary directory must exist");
    let home = directory.path().join("project");
    let status = ProjectService::new()
        .create_project("项目记忆测试", home.clone())
        .expect("project must initialize");
    MemoryFixture {
        _directory: directory,
        home,
        project_id: status.project_id,
    }
}

fn append(fixture: &MemoryFixture, kind: DomainEventKind) -> DomainEvent {
    let mut database = Database::open(&fixture.home.join(".coldstart").join("state.db"))
        .expect("database must open");
    let event = DomainEvent::new(fixture.project_id, kind);
    database.append_event(&event).expect("event must append");
    event
}

fn all_event_kinds() -> Vec<DomainEventKind> {
    let work_item_id = WorkItemId::new();
    let report_id = WorkReportId::new();
    let repository_id = RepositoryId::new();
    vec![
        DomainEventKind::ProjectInitialized {
            name: "订单系统".to_owned(),
        },
        DomainEventKind::ArtifactImported {
            artifact_id: ArtifactId::new(),
            file_name: "需求说明.md".to_owned(),
            content_hash: "a".repeat(64),
        },
        DomainEventKind::BlueprintDraftCreated {
            revision_id: BlueprintRevisionId::new(),
            revision_number: 1,
            generation_source: GenerationSource::Model,
        },
        DomainEventKind::BlueprintPublished {
            revision_id: BlueprintRevisionId::new(),
            revision_number: 1,
        },
        DomainEventKind::InfoRequestCreated {
            request_id: InfoRequestId::new(),
            question: "OSS 使用哪个地域？".to_owned(),
        },
        DomainEventKind::InfoRequestResolved {
            request_id: InfoRequestId::new(),
            fact_id: ProjectFactId::new(),
            version: 1,
        },
        DomainEventKind::PlanDraftCreated {
            revision_id: PlanRevisionId::new(),
            revision_number: 1,
            task_count: 8,
        },
        DomainEventKind::PlanPublished {
            revision_id: PlanRevisionId::new(),
            revision_number: 1,
            task_count: 8,
        },
        DomainEventKind::PlanSuperseded {
            revision_id: PlanRevisionId::new(),
            superseded_by: PlanRevisionId::new(),
        },
        DomainEventKind::WorkClaimed {
            work_item_id,
            task_key: "slice:server:order-api".to_owned(),
            lease_id: LeaseId::new(),
            agent_profile: "codex".to_owned(),
        },
        DomainEventKind::WorkStarted { work_item_id },
        DomainEventKind::WorkHeartbeat {
            work_item_id,
            lease_id: LeaseId::new(),
            expires_at: Utc::now(),
        },
        DomainEventKind::WorkBlocked {
            work_item_id,
            lease_id: LeaseId::new(),
            reason: "等待小程序主体资质".to_owned(),
        },
        DomainEventKind::WorkReportSubmitted {
            work_item_id,
            report_id,
            accepted: true,
        },
        DomainEventKind::WorkCompleted {
            work_item_id,
            task_key: "slice:server:order-api".to_owned(),
        },
        DomainEventKind::VerificationStarted {
            work_item_id,
            report_id,
            snapshot_hash: "b".repeat(64),
        },
        DomainEventKind::VerificationPassed {
            work_item_id,
            report_id,
            snapshot_hash: "b".repeat(64),
        },
        DomainEventKind::VerificationFailed {
            work_item_id,
            report_id,
            command_name: Some("web-test".to_owned()),
            failure_kind: "non_zero_exit".to_owned(),
        },
        DomainEventKind::VerificationInterrupted {
            work_item_id,
            report_id,
        },
        DomainEventKind::ReviewStarted {
            work_item_id,
            report_id,
            snapshot_hash: "c".repeat(64),
        },
        DomainEventKind::ReviewPassed {
            work_item_id,
            report_id,
            snapshot_hash: "c".repeat(64),
            finding_count: 0,
        },
        DomainEventKind::ReviewBlocked {
            work_item_id,
            report_id,
            snapshot_hash: "c".repeat(64),
            blocking_finding_count: 1,
        },
        DomainEventKind::ReviewFailed {
            work_item_id,
            report_id,
            failure_code: "review_response_invalid".to_owned(),
        },
        DomainEventKind::ReviewInterrupted {
            work_item_id,
            report_id,
        },
        DomainEventKind::ReviewWaived {
            work_item_id,
            report_id,
            waived_by: "项目负责人".to_owned(),
        },
        DomainEventKind::WorkReleased {
            work_item_id,
            lease_id: LeaseId::new(),
            reason: "切换任务".to_owned(),
        },
        DomainEventKind::LeaseExpired {
            work_item_id,
            lease_id: LeaseId::new(),
        },
        DomainEventKind::HumanWorkCompleted {
            work_item_id,
            accepted: true,
        },
        DomainEventKind::RepositorySaved {
            repository_id,
            repository_key: "web".to_owned(),
            remote_url: "https://example.invalid/web.git".to_owned(),
        },
        DomainEventKind::RepositoryVerificationCommandsUpdated {
            repository_id,
            repository_key: "web".to_owned(),
            command_count: 2,
        },
        DomainEventKind::CredentialSaved {
            credential_id: CredentialId::new(),
            provider: "aliyun_oss".to_owned(),
            environment: "development".to_owned(),
        },
        DomainEventKind::CredentialDeleted {
            credential_id: CredentialId::new(),
        },
        DomainEventKind::CapabilityUpdated {
            capability_id: "capability_oss".to_owned(),
            environment: "development".to_owned(),
            status: "verified".to_owned(),
        },
        DomainEventKind::GitSyncRecorded {
            sync_id: GitSyncId::new(),
            work_item_id,
            repository_id,
            status: "pushed".to_owned(),
        },
        DomainEventKind::ProjectSettingUpdated {
            setting_key: "review.configuration".to_owned(),
        },
    ]
}

#[test]
fn projects_every_domain_event_into_a_complete_chinese_timeline_item() {
    let fixture = fixture();
    let kinds = all_event_kinds();
    for kind in &kinds {
        append(&fixture, kind.clone());
    }

    let page = ProjectMemoryService::new()
        .read(fixture.home.clone(), None, Some(500), true)
        .expect("memory page must load");

    let mut seen_types = std::collections::BTreeSet::new();
    for kind in &kinds {
        let item = page
            .items
            .iter()
            .find(|item| item.event_type == kind.type_name())
            .unwrap_or_else(|| panic!("missing memory item for {}", kind.type_name()));
        assert!(
            !item.title.trim().is_empty(),
            "{} title is empty",
            kind.type_name()
        );
        assert!(
            !item.summary.trim().is_empty(),
            "{} summary is empty",
            kind.type_name()
        );
        assert!(!item.is_heartbeat || kind.type_name() == "work_heartbeat");
        seen_types.insert(kind.type_name());
    }

    assert_eq!(seen_types.len(), all_event_kinds().len());
    let phases = page
        .phase_counts
        .iter()
        .map(|entry| entry.phase)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        phases,
        std::collections::BTreeSet::from([
            MemoryPhase::RequirementsBlueprint,
            MemoryPhase::PlanExecution,
            MemoryPhase::VerificationReview,
            MemoryPhase::RepositoryCapability,
            MemoryPhase::SystemConfiguration,
        ])
    );
    assert_eq!(
        page.phase_counts
            .iter()
            .map(|entry| entry.count)
            .sum::<u64>(),
        page.visible_count
    );
    let expected_counts = [
        (MemoryPhase::RequirementsBlueprint, 7),
        (MemoryPhase::PlanExecution, 11),
        (MemoryPhase::VerificationReview, 10),
        (MemoryPhase::RepositoryCapability, 6),
        (MemoryPhase::SystemConfiguration, 1),
    ];
    for (phase, expected) in expected_counts {
        assert_eq!(
            page.phase_counts
                .iter()
                .find(|entry| entry.phase == phase)
                .map(|entry| entry.count),
            Some(expected),
            "unexpected event count for {phase:?}"
        );
    }
}

#[test]
fn hides_heartbeats_by_default_and_reports_accurate_counts() {
    let fixture = fixture();
    append(
        &fixture,
        DomainEventKind::WorkHeartbeat {
            work_item_id: WorkItemId::new(),
            lease_id: LeaseId::new(),
            expires_at: Utc::now(),
        },
    );
    append(
        &fixture,
        DomainEventKind::WorkHeartbeat {
            work_item_id: WorkItemId::new(),
            lease_id: LeaseId::new(),
            expires_at: Utc::now(),
        },
    );
    append(
        &fixture,
        DomainEventKind::PlanPublished {
            revision_id: PlanRevisionId::new(),
            revision_number: 1,
            task_count: 4,
        },
    );

    let visible = ProjectMemoryService::new()
        .read(fixture.home.clone(), None, Some(100), false)
        .expect("visible memory must load");
    let complete = ProjectMemoryService::new()
        .read(fixture.home.clone(), None, Some(100), true)
        .expect("complete memory must load");

    assert_eq!(visible.total_count, 4);
    assert_eq!(visible.visible_count, 2);
    assert_eq!(visible.heartbeat_count, 2);
    assert_eq!(visible.items.len(), 2);
    assert!(visible.items.iter().all(|item| !item.is_heartbeat));
    assert_eq!(complete.items.len(), 4);
    assert_eq!(
        complete
            .items
            .iter()
            .filter(|item| item.is_heartbeat)
            .count(),
        2
    );
    assert_eq!(
        visible
            .phase_counts
            .iter()
            .map(|entry| entry.count)
            .sum::<u64>(),
        visible.visible_count
    );
}

#[test]
fn safe_projection_never_exposes_remote_credentials_lease_ids_or_payloads() {
    let fixture = fixture();
    let lease_id = LeaseId::new();
    let repository_id = RepositoryId::new();
    let remote_url = "https://oauth2:top-secret-token@example.invalid/team/private.git";
    append(
        &fixture,
        DomainEventKind::RepositorySaved {
            repository_id,
            repository_key: "web".to_owned(),
            remote_url: remote_url.to_owned(),
        },
    );
    append(
        &fixture,
        DomainEventKind::WorkClaimed {
            work_item_id: WorkItemId::new(),
            task_key: "slice:web:checkout".to_owned(),
            lease_id,
            agent_profile: "codex".to_owned(),
        },
    );

    let page = ProjectMemoryService::new()
        .read(fixture.home.clone(), None, Some(100), true)
        .expect("memory page must load");
    let serialized = serde_json::to_string(&page).expect("memory page must serialize");

    assert!(!serialized.contains("top-secret-token"));
    assert!(!serialized.contains(remote_url));
    assert!(!serialized.contains(&lease_id.to_string()));
    assert!(!serialized.contains("payload_json"));
    assert!(!serialized.contains("payloadJson"));
    assert!(serialized.contains(&repository_id.to_string()));
    assert!(serialized.contains("slice:web:checkout"));
}

#[test]
fn rejects_invalid_page_bounds_with_stable_codes() {
    let fixture = fixture();
    let service = ProjectMemoryService::new();

    let zero_limit = service
        .read(fixture.home.clone(), None, Some(0), false)
        .expect_err("zero limit must fail");
    let oversized_limit = service
        .read(fixture.home.clone(), None, Some(501), false)
        .expect_err("oversized limit must fail");
    let invalid_cursor = service
        .read(fixture.home.clone(), Some(0), Some(100), false)
        .expect_err("zero cursor must fail");

    assert_eq!(zero_limit.code(), "invalid_memory_limit");
    assert_eq!(oversized_limit.code(), "invalid_memory_limit");
    assert_eq!(invalid_cursor.code(), "invalid_memory_cursor");
}
