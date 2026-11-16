//! 历史代码调查状态机的存储集成测试。
//!
//! 测试覆盖主动领取、结构化提交、人工确认、租约过期、幂等冲突和架构迁移，
//! 同时确认一次性领取令牌不会通过查询投影泄漏。

use std::path::PathBuf;

use chrono::{Duration, Utc};
use coldstart_core::{
    CodeFinding, CodeInvestigation, CodeInvestigationClaim, CodeInvestigationReport,
    CodeInvestigationStatus, Project, Repository, RepositoryAuthMode, RepositorySyncPolicy,
};
use coldstart_storage::{Database, StorageError};
use rusqlite::Connection;
use tempfile::TempDir;

struct Fixture {
    directory: TempDir,
    database: Database,
    project: Project,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("temporary directory must exist");
        let project = Project::new("历史项目", directory.path().join("project"))
            .expect("project must be valid");
        let mut database = Database::open(&directory.path().join(".coldstart/state.db"))
            .expect("database must open");
        database
            .initialize_project(&project)
            .expect("project must initialize");
        database
            .save_repository(&repository(project.id, directory.path().join("server")))
            .expect("repository must save");
        Self {
            directory,
            database,
            project,
        }
    }

    fn create(&mut self) -> CodeInvestigation {
        self.database
            .create_code_investigation(
                &self.project.id,
                "server",
                "main",
                "现有订单创建流程有哪些扩展点和风险？",
                Utc::now(),
            )
            .expect("investigation must create")
    }

    fn claim(&mut self, now: chrono::DateTime<Utc>) -> CodeInvestigationClaim {
        self.database
            .claim_next_code_investigation(&self.project.id, Some("server"), "codex", now)
            .expect("claim must run")
            .expect("an investigation must be available")
    }
}

fn repository(project_id: coldstart_core::ProjectId, path: PathBuf) -> Repository {
    Repository::new(
        project_id,
        "server".to_owned(),
        "服务端".to_owned(),
        path,
        "https://example.invalid/server.git".to_owned(),
        "main".to_owned(),
        vec!["main".to_owned()],
        RepositoryAuthMode::None,
        None,
        RepositorySyncPolicy::Manual,
    )
    .expect("repository must validate")
}

fn report(idempotency_key: &str) -> CodeInvestigationReport {
    CodeInvestigationReport {
        commit_sha: "a".repeat(40),
        summary: "订单模块已有可复用的库存服务，变更应沿现有事务边界扩展".to_owned(),
        findings: vec![CodeFinding {
            path: "src/orders/service.rs".to_owned(),
            line_start: 20,
            line_end: 48,
            symbol: Some("create_order".to_owned()),
            summary: "订单创建会调用库存扣减服务".to_owned(),
            risk: "新增优惠逻辑可能绕过库存一致性检查".to_owned(),
            suggested_change_paths: vec!["src/orders/service.rs".to_owned()],
        }],
        limitations: vec!["未检查生产环境配置".to_owned()],
        idempotency_key: idempotency_key.to_owned(),
    }
}

#[test]
fn claims_submits_and_confirms_a_structured_report() {
    let mut fixture = Fixture::new();
    let created = fixture.create();
    assert_eq!(created.status, CodeInvestigationStatus::Requested);

    let now = Utc::now();
    let claim = fixture.claim(now);
    assert_eq!(claim.investigation.status, CodeInvestigationStatus::Claimed);
    assert_eq!(claim.investigation.agent_profile.as_deref(), Some("codex"));
    assert_eq!(
        claim.investigation.lease_expires_at,
        Some(now + Duration::minutes(60))
    );

    let submitted = fixture
        .database
        .submit_code_investigation(
            &fixture.project.id,
            created.id,
            &claim.claim_token,
            report("investigation-report-1"),
            now,
        )
        .expect("report must submit");
    assert_eq!(submitted.status, CodeInvestigationStatus::Submitted);
    assert_eq!(submitted.findings.len(), 1);
    assert!(submitted.report_hash.is_some());

    let confirmed = fixture
        .database
        .confirm_code_investigation(&fixture.project.id, created.id, now)
        .expect("submitted report must confirm");
    assert_eq!(confirmed.status, CodeInvestigationStatus::Confirmed);
    assert!(confirmed.confirmed_at.is_some());
}

#[test]
fn rejects_wrong_tokens_expired_leases_and_conflicting_duplicate_reports() {
    let mut fixture = Fixture::new();
    let created = fixture.create();
    let now = Utc::now();
    let claim = fixture.claim(now);

    let error = fixture
        .database
        .submit_code_investigation(
            &fixture.project.id,
            created.id,
            "wrong-token",
            report("wrong-token-report"),
            now,
        )
        .expect_err("wrong token must fail");
    assert!(matches!(
        error,
        StorageError::CodeInvestigationClaimInvalid(id) if id == created.id
    ));

    let error = fixture
        .database
        .submit_code_investigation(
            &fixture.project.id,
            created.id,
            &claim.claim_token,
            report("expired-report"),
            now + Duration::minutes(61),
        )
        .expect_err("expired lease must fail");
    assert!(matches!(
        error,
        StorageError::CodeInvestigationLeaseExpired(id) if id == created.id
    ));

    let reclaimed = fixture.claim(now + Duration::minutes(62));
    assert_eq!(reclaimed.investigation.id, created.id);
    let submitted = fixture
        .database
        .submit_code_investigation(
            &fixture.project.id,
            created.id,
            &reclaimed.claim_token,
            report("duplicate-report"),
            now + Duration::minutes(62),
        )
        .expect("reclaimed report must submit");
    assert_eq!(submitted.status, CodeInvestigationStatus::Submitted);

    let repeated = fixture
        .database
        .submit_code_investigation(
            &fixture.project.id,
            created.id,
            &reclaimed.claim_token,
            report("duplicate-report"),
            now + Duration::minutes(63),
        )
        .expect("identical duplicate must be idempotent");
    assert_eq!(repeated, submitted);

    let mut conflicting = report("duplicate-report");
    conflicting.summary = "不同的调查结论".to_owned();
    let error = fixture
        .database
        .submit_code_investigation(
            &fixture.project.id,
            created.id,
            &reclaimed.claim_token,
            conflicting,
            now + Duration::minutes(63),
        )
        .expect_err("different report content must conflict");
    assert!(matches!(
        error,
        StorageError::CodeInvestigationReportConflict(id) if id == created.id
    ));
}

#[test]
fn rejected_reports_cannot_be_confirmed() {
    let mut fixture = Fixture::new();
    let created = fixture.create();
    let now = Utc::now();
    let claim = fixture.claim(now);
    fixture
        .database
        .submit_code_investigation(
            &fixture.project.id,
            created.id,
            &claim.claim_token,
            report("rejected-report"),
            now,
        )
        .expect("report must submit");

    let error = fixture
        .database
        .reject_code_investigation(&fixture.project.id, created.id, " ")
        .expect_err("empty rejection reason must fail");
    assert!(matches!(
        error,
        StorageError::EmptyCodeInvestigationRejection
    ));

    let rejected = fixture
        .database
        .reject_code_investigation(
            &fixture.project.id,
            created.id,
            "证据不足，需要补充入口分析",
        )
        .expect("report must reject");
    assert_eq!(rejected.status, CodeInvestigationStatus::Rejected);
    assert_eq!(
        rejected.rejection_reason.as_deref(),
        Some("证据不足，需要补充入口分析")
    );

    let error = fixture
        .database
        .confirm_code_investigation(&fixture.project.id, created.id, now)
        .expect_err("rejected report must not confirm");
    assert!(matches!(
        error,
        StorageError::CodeInvestigationNotSubmitted(id) if id == created.id
    ));
}

#[test]
fn claim_tokens_and_token_hashes_are_not_projected() {
    let mut fixture = Fixture::new();
    fixture.create();
    let claim = fixture.claim(Utc::now());

    let list = fixture
        .database
        .list_code_investigations(&fixture.project.id)
        .expect("investigations must list");
    let projected = serde_json::to_string(&list).expect("projection must serialize");
    assert!(!projected.contains(&claim.claim_token));
    assert!(!projected.contains("claimToken"));
    assert!(!projected.contains("claim_token_hash"));

    let database_path = fixture.directory.path().join(".coldstart/state.db");
    drop(fixture.database);
    let connection = Connection::open(database_path).expect("database must open directly");
    let stored_hash: String = connection
        .query_row(
            "SELECT claim_token_hash FROM code_investigations LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("token hash must exist");
    assert_eq!(stored_hash.len(), 64);
    assert_ne!(stored_hash, claim.claim_token);
}

#[test]
fn migrates_schema_twelve_to_thirteen_without_losing_project_data() {
    let fixture = Fixture::new();
    let database_path = fixture.directory.path().join(".coldstart/state.db");
    let project_id = fixture.project.id;
    let events_before = fixture
        .database
        .event_count(&project_id)
        .expect("event count must load");
    drop(fixture.database);

    let connection = Connection::open(&database_path).expect("database must open directly");
    connection
        .execute_batch(
            "DROP TABLE code_investigations;
             PRAGMA user_version = 12;",
        )
        .expect("v12 state must be restored");
    drop(connection);

    let mut database = Database::open(&database_path).expect("schema twelve must migrate");
    assert_eq!(database.schema_version().expect("schema version"), 13);
    assert_eq!(database.event_count(&project_id).unwrap(), events_before);
    assert!(
        database
            .repository_by_key(&project_id, "server")
            .expect("repository query must work")
            .is_some()
    );
    database
        .create_code_investigation(
            &project_id,
            "server",
            "main",
            "迁移后是否可以创建调查？",
            Utc::now(),
        )
        .expect("investigation must create after migration");
}
