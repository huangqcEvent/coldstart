//! 仓库验证命令与单次验证运行的持久化。
//!
//! 验证开始、成功、失败和租约消耗由同一状态机驱动；命令快照与执行结果分离，
//! 确保重试使用冻结配置而不是读取后的新配置。

use chrono::Duration;
use chrono::{DateTime, Utc};
use coldstart_core::{
    DomainEvent, DomainEventKind, LeaseId, ProjectId, RepositoryId, RepositoryVerificationCommand,
    VerificationFailureKind, VerificationResult, VerificationSnapshot, VerificationStatus,
    WorkItemId, WorkReportId, WorkReportInput, WorkReportRecord, WorkRunId,
    normalize_verification_commands, validate_work_report,
};
use rusqlite::{OptionalExtension, Row, TransactionBehavior, params};

use crate::{
    Database, StorageError,
    database::{insert_event, project_exists},
    plan_store::{
        WorkReportInsert, authorize_in_tx, insert_work_report, query_report_by_id,
        query_report_by_idempotency, refresh_work_items,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingVerification {
    pub report: WorkReportRecord,
    pub snapshot: VerificationSnapshot,
    pub review: Option<coldstart_core::ReviewSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeginVerification {
    Started(Box<PendingVerification>),
    AlreadyResolved(Box<WorkReportRecord>),
}

impl Database {
    /// 原子替换仓库验证命令并重建有序快照，保留命令 ID 与边界。
    pub fn replace_repository_verification_commands(
        &mut self,
        project_id: &ProjectId,
        repository_key: &str,
        commands: Vec<RepositoryVerificationCommand>,
    ) -> Result<Vec<RepositoryVerificationCommand>, StorageError> {
        let commands = normalize_verification_commands(commands)
            .map_err(|error| StorageError::VerificationCommandInvalid(error.to_string()))?;
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !project_exists(&transaction, project_id)? {
            return Err(StorageError::ProjectNotFound(*project_id));
        }
        let repository_id = repository_id_by_key(&transaction, project_id, repository_key)?;
        transaction.execute(
            "DELETE FROM repository_verification_commands WHERE repository_id = ?1",
            [repository_id.to_string()],
        )?;
        let now = Utc::now().to_rfc3339();
        for command in &commands {
            transaction.execute(
                "INSERT INTO repository_verification_commands (
                    repository_id, name, executable, args_json, timeout_seconds,
                    position, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                params![
                    repository_id.to_string(),
                    command.name,
                    command.executable.to_string_lossy(),
                    serde_json::to_string(&command.args)?,
                    command.timeout_seconds,
                    command.position,
                    now,
                ],
            )?;
        }
        insert_event(
            &transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::RepositoryVerificationCommandsUpdated {
                    repository_id,
                    repository_key: repository_key.to_owned(),
                    command_count: commands.len(),
                },
            ),
        )?;
        transaction.commit()?;
        Ok(commands)
    }

    pub fn list_repository_verification_commands(
        &self,
        project_id: &ProjectId,
        repository_key: &str,
    ) -> Result<Vec<RepositoryVerificationCommand>, StorageError> {
        let connection = self.connection();
        let repository_id = repository_id_by_key(connection, project_id, repository_key)?;
        commands_for_repository(connection, &repository_id)
    }

    /// 冻结当前验证命令并启动受控运行，推进工作项验证状态。
    pub fn begin_work_verification(
        &mut self,
        work_item_id: &WorkItemId,
        lease_token: &str,
        report: WorkReportInput,
        now: DateTime<Utc>,
        lease_duration: Duration,
    ) -> Result<BeginVerification, StorageError> {
        if report.idempotency_key.trim().is_empty() {
            return Err(StorageError::ReportRejected(
                "report idempotency key cannot be empty".to_owned(),
            ));
        }
        let idempotency_hash = coldstart_core::sha256_hex(report.idempotency_key.trim().as_bytes());
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(existing) =
            query_report_by_idempotency(&transaction, work_item_id, &idempotency_hash)?
        {
            if existing.verification_status == VerificationStatus::Running {
                return Err(StorageError::VerificationInProgress(*work_item_id));
            }
            transaction.commit()?;
            return Ok(BeginVerification::AlreadyResolved(Box::new(existing)));
        }

        let authorization = authorize_in_tx(
            &transaction,
            work_item_id,
            lease_token,
            now,
            lease_duration,
            false,
        )?;
        if let Err(error) = validate_work_report(&authorization.task_spec, &report) {
            let record = insert_work_report(
                &transaction,
                work_item_id,
                &authorization.run.id,
                WorkReportInsert {
                    report: &report,
                    idempotency_hash: &idempotency_hash,
                    accepted: false,
                    rejection_reasons: vec![error.to_string()],
                    verification_status: VerificationStatus::NotRequired,
                    verification: None,
                },
                now,
            )?;
            insert_event(
                &transaction,
                &DomainEvent::new(
                    authorization.work_item.project_id,
                    DomainEventKind::WorkReportSubmitted {
                        work_item_id: *work_item_id,
                        report_id: record.id,
                        accepted: false,
                    },
                ),
            )?;
            transaction.commit()?;
            return Err(StorageError::ReportRejected(error.to_string()));
        }

        let snapshot = authorization
            .run
            .verification_snapshot
            .clone()
            .ok_or(StorageError::VerificationNotConfigured(*work_item_id))?;
        let review = authorization.run.review_snapshot.clone();
        let snapshot_hash = snapshot.content_hash();
        if authorization.run.verification_snapshot_hash.as_deref() != Some(snapshot_hash.as_str()) {
            return Err(StorageError::VerificationCommandInvalid(
                "verification snapshot hash does not match the persisted snapshot".to_owned(),
            ));
        }
        let verification = VerificationResult {
            snapshot_hash,
            status: VerificationStatus::Running,
            started_at: now,
            finished_at: None,
            total_duration_ms: None,
            failure_kind: None,
            failure_command: None,
            commands: Vec::new(),
        };
        let record = insert_work_report(
            &transaction,
            work_item_id,
            &authorization.run.id,
            WorkReportInsert {
                report: &report,
                idempotency_hash: &idempotency_hash,
                accepted: false,
                rejection_reasons: Vec::new(),
                verification_status: VerificationStatus::Running,
                verification: Some(verification.clone()),
            },
            now,
        )?;
        let verification_expiry =
            now + Duration::seconds(i64::from(snapshot.total_timeout_seconds) + 300);
        transaction.execute(
            "UPDATE leases SET last_heartbeat_at = ?1, expires_at = ?2 WHERE id = ?3",
            params![
                now.to_rfc3339(),
                verification_expiry.to_rfc3339(),
                authorization.lease.id.to_string()
            ],
        )?;
        transaction.execute(
            "UPDATE work_runs SET status = 'submitted' WHERE id = ?1",
            [authorization.run.id.to_string()],
        )?;
        transaction.execute(
            "UPDATE work_items SET status = 'submitted', updated_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), work_item_id.to_string()],
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                authorization.work_item.project_id,
                DomainEventKind::VerificationStarted {
                    work_item_id: *work_item_id,
                    report_id: record.id,
                    snapshot_hash: verification.snapshot_hash,
                },
            ),
        )?;
        transaction.commit()?;
        Ok(BeginVerification::Started(Box::new(PendingVerification {
            report: record,
            snapshot,
            review,
        })))
    }

    /// 记录验证成功、输出摘要与租约消费，并解锁后续 Review。
    pub fn finish_work_verification_success(
        &mut self,
        project_id: &ProjectId,
        work_item_id: &WorkItemId,
        report_id: &WorkReportId,
        verification: VerificationResult,
        now: DateTime<Utc>,
    ) -> Result<WorkReportRecord, StorageError> {
        if verification.status != VerificationStatus::Passed {
            return Err(StorageError::VerificationCommandInvalid(
                "successful verification result must have passed status".to_owned(),
            ));
        }
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = query_report_by_id(&transaction, report_id)?
            .ok_or_else(|| StorageError::ReportRejected("work report does not exist".to_owned()))?;
        if existing.work_item_id != *work_item_id {
            return Err(StorageError::ReportRejected(
                "work report does not belong to the requested work item".to_owned(),
            ));
        }
        if existing.verification_status == VerificationStatus::Passed {
            transaction.commit()?;
            return Ok(existing);
        }
        if existing.verification_status != VerificationStatus::Running {
            return Err(StorageError::ReportRejected(
                "work report is not running verification".to_owned(),
            ));
        }
        let (run_id, lease_id, plan_revision_id, task_key): (String, String, String, String) =
            transaction.query_row(
                "SELECT r.id, r.lease_id, w.plan_revision_id, w.task_key
             FROM work_runs r JOIN work_items w ON w.id = r.work_item_id
             WHERE r.id = ?1 AND w.id = ?2 AND w.project_id = ?3",
                params![
                    existing.work_run_id.to_string(),
                    work_item_id.to_string(),
                    project_id.to_string()
                ],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
        let run_id: WorkRunId = parse_id(&run_id, "work run")?;
        let lease_id: LeaseId = parse_id(&lease_id, "lease")?;
        let plan_revision_id: coldstart_core::PlanRevisionId =
            parse_id(&plan_revision_id, "plan revision")?;
        update_verification_report(&transaction, report_id, &verification, true, Vec::new())?;
        transaction.execute(
            "UPDATE leases SET status = 'consumed', released_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), lease_id.to_string()],
        )?;
        transaction.execute(
            "UPDATE work_runs SET status = 'completed', ended_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), run_id.to_string()],
        )?;
        transaction.execute(
            "UPDATE work_items
             SET status = 'done', updated_at = ?1, completed_at = ?1
             WHERE id = ?2",
            params![now.to_rfc3339(), work_item_id.to_string()],
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::VerificationPassed {
                    work_item_id: *work_item_id,
                    report_id: *report_id,
                    snapshot_hash: verification.snapshot_hash.clone(),
                },
            ),
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::WorkReportSubmitted {
                    work_item_id: *work_item_id,
                    report_id: *report_id,
                    accepted: true,
                },
            ),
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::WorkCompleted {
                    work_item_id: *work_item_id,
                    task_key,
                },
            ),
        )?;
        refresh_work_items(&transaction, project_id, &plan_revision_id, now)?;
        let record = query_report_by_id(&transaction, report_id)?
            .ok_or_else(|| StorageError::ReportRejected("work report disappeared".to_owned()))?;
        transaction.commit()?;
        Ok(record)
    }

    /// 记录验证失败并保留租约供修复重试，不误推进完成状态。
    pub fn finish_work_verification_failure(
        &mut self,
        project_id: &ProjectId,
        work_item_id: &WorkItemId,
        report_id: &WorkReportId,
        verification: VerificationResult,
        now: DateTime<Utc>,
        lease_duration: Duration,
    ) -> Result<WorkReportRecord, StorageError> {
        if verification.status != VerificationStatus::Failed {
            return Err(StorageError::VerificationCommandInvalid(
                "failed verification result must have failed status".to_owned(),
            ));
        }
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = query_report_by_id(&transaction, report_id)?
            .ok_or_else(|| StorageError::ReportRejected("work report does not exist".to_owned()))?;
        if existing.work_item_id != *work_item_id
            || existing.verification_status != VerificationStatus::Running
        {
            return Err(StorageError::ReportRejected(
                "work report is not running verification for this work item".to_owned(),
            ));
        }
        let (run_id, lease_id): (String, String) = transaction.query_row(
            "SELECT r.id, r.lease_id
             FROM work_runs r JOIN work_items w ON w.id = r.work_item_id
             WHERE r.id = ?1 AND w.id = ?2 AND w.project_id = ?3",
            params![
                existing.work_run_id.to_string(),
                work_item_id.to_string(),
                project_id.to_string()
            ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let run_id: WorkRunId = parse_id(&run_id, "work run")?;
        let lease_id: LeaseId = parse_id(&lease_id, "lease")?;
        let reason = verification_failure_reason(&verification);
        update_verification_report(&transaction, report_id, &verification, false, vec![reason])?;
        let expires_at = now + lease_duration;
        let updated = transaction.execute(
            "UPDATE leases SET status = 'active', last_heartbeat_at = ?1, expires_at = ?2
             WHERE id = ?3 AND status = 'active'",
            params![
                now.to_rfc3339(),
                expires_at.to_rfc3339(),
                lease_id.to_string()
            ],
        )?;
        if updated == 0 {
            return Err(StorageError::LeaseExpired(*work_item_id));
        }
        transaction.execute(
            "UPDATE work_runs SET status = 'running', ended_at = NULL WHERE id = ?1",
            [run_id.to_string()],
        )?;
        transaction.execute(
            "UPDATE work_items SET status = 'in_progress', updated_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), work_item_id.to_string()],
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::VerificationFailed {
                    work_item_id: *work_item_id,
                    report_id: *report_id,
                    command_name: verification.failure_command.clone(),
                    failure_kind: verification
                        .failure_kind
                        .map(failure_kind_str)
                        .unwrap_or("unknown")
                        .to_owned(),
                },
            ),
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::WorkReportSubmitted {
                    work_item_id: *work_item_id,
                    report_id: *report_id,
                    accepted: false,
                },
            ),
        )?;
        let record = query_report_by_id(&transaction, report_id)?
            .ok_or_else(|| StorageError::ReportRejected("work report disappeared".to_owned()))?;
        transaction.commit()?;
        Ok(record)
    }
}

pub(crate) fn interrupt_running_verification(
    transaction: &rusqlite::Transaction<'_>,
    work_item_id: &WorkItemId,
    now: DateTime<Utc>,
) -> Result<(), StorageError> {
    let Some(report_id) = report_id_for_work_item(transaction, work_item_id)? else {
        return Ok(());
    };
    let Some(mut record) = query_report_by_id(transaction, &report_id)? else {
        return Ok(());
    };
    if record.verification_status != VerificationStatus::Running {
        return Ok(());
    }
    let mut verification = record.verification.take().unwrap_or(VerificationResult {
        snapshot_hash: String::new(),
        status: VerificationStatus::Running,
        started_at: now,
        finished_at: None,
        total_duration_ms: None,
        failure_kind: None,
        failure_command: None,
        commands: Vec::new(),
    });
    verification.status = VerificationStatus::Interrupted;
    verification.finished_at = Some(now);
    verification.total_duration_ms =
        Some((now - verification.started_at).num_milliseconds().max(0) as u64);
    verification.failure_kind = Some(VerificationFailureKind::Interrupted);
    update_verification_report(
        transaction,
        &report_id,
        &verification,
        false,
        vec!["验证因租约过期或程序中断而未完成".to_owned()],
    )?;
    insert_event(
        transaction,
        &DomainEvent::new(
            record_work_item_project(transaction, work_item_id)?,
            DomainEventKind::VerificationInterrupted {
                work_item_id: *work_item_id,
                report_id,
            },
        ),
    )?;
    Ok(())
}

pub(crate) fn update_verification_report(
    transaction: &rusqlite::Transaction<'_>,
    report_id: &WorkReportId,
    verification: &VerificationResult,
    accepted: bool,
    rejection_reasons: Vec<String>,
) -> Result<(), StorageError> {
    let updated = transaction.execute(
        "UPDATE work_reports SET accepted = ?1, rejection_reasons_json = ?2,
            verification_status = ?3, verification_json = ?4,
            verification_finished_at = ?5
         WHERE id = ?6 AND verification_status = 'running'",
        params![
            i64::from(accepted),
            serde_json::to_string(&rejection_reasons)?,
            verification_status_str(verification.status),
            serde_json::to_string(verification)?,
            verification.finished_at.map(|value| value.to_rfc3339()),
            report_id.to_string(),
        ],
    )?;
    if updated == 0 {
        return Err(StorageError::ReportRejected(
            "work report is no longer running verification".to_owned(),
        ));
    }
    Ok(())
}

fn verification_failure_reason(verification: &VerificationResult) -> String {
    let kind = verification
        .failure_kind
        .map(failure_kind_str)
        .unwrap_or("unknown");
    match &verification.failure_command {
        Some(command) => format!("本地验证失败：{command}（{kind}）"),
        None => format!("本地验证失败（{kind}）"),
    }
}

fn failure_kind_str(kind: VerificationFailureKind) -> &'static str {
    match kind {
        VerificationFailureKind::NonZeroExit => "non_zero_exit",
        VerificationFailureKind::LaunchFailed => "launch_failed",
        VerificationFailureKind::Timeout => "timeout",
        VerificationFailureKind::GitInvariant => "git_invariant",
        VerificationFailureKind::Infrastructure => "infrastructure",
        VerificationFailureKind::Interrupted => "interrupted",
    }
}

pub(crate) fn record_work_item_project(
    transaction: &rusqlite::Transaction<'_>,
    work_item_id: &WorkItemId,
) -> Result<ProjectId, StorageError> {
    let project_id = transaction.query_row(
        "SELECT project_id FROM work_items WHERE id = ?1",
        [work_item_id.to_string()],
        |row| row.get::<_, String>(0),
    )?;
    parse_id(&project_id, "project")
}

fn parse_id<T>(value: &str, kind: &'static str) -> Result<T, StorageError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    value.parse().map_err(|_| StorageError::InvalidId {
        kind,
        value: value.to_owned(),
    })
}

pub(crate) fn repository_id_by_key(
    connection: &rusqlite::Connection,
    project_id: &ProjectId,
    repository_key: &str,
) -> Result<RepositoryId, StorageError> {
    let id = connection
        .query_row(
            "SELECT id FROM repositories WHERE project_id = ?1 AND repository_key = ?2",
            params![project_id.to_string(), repository_key],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or_else(|| StorageError::RepositoryNotFound(repository_key.to_owned()))?;
    id.parse().map_err(|_| StorageError::InvalidId {
        kind: "repository",
        value: id,
    })
}

pub(crate) fn commands_for_repository(
    connection: &rusqlite::Connection,
    repository_id: &RepositoryId,
) -> Result<Vec<RepositoryVerificationCommand>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT name, executable, args_json, timeout_seconds, position
         FROM repository_verification_commands
         WHERE repository_id = ?1
         ORDER BY position ASC",
    )?;
    let rows = statement.query_map([repository_id.to_string()], read_command_row)?;
    let mut commands = Vec::new();
    for row in rows {
        commands.push(row?.into_command()?);
    }
    Ok(commands)
}

struct StoredVerificationCommandRow {
    name: String,
    executable: String,
    args_json: String,
    timeout_seconds: i64,
    position: i64,
}

impl StoredVerificationCommandRow {
    fn into_command(self) -> Result<RepositoryVerificationCommand, StorageError> {
        Ok(RepositoryVerificationCommand {
            name: self.name,
            executable: self.executable.into(),
            args: serde_json::from_str(&self.args_json)?,
            timeout_seconds: u32::try_from(self.timeout_seconds).map_err(|_| {
                StorageError::VerificationCommandInvalid(
                    "stored timeout is outside the supported range".to_owned(),
                )
            })?,
            position: u32::try_from(self.position).map_err(|_| {
                StorageError::VerificationCommandInvalid(
                    "stored position is outside the supported range".to_owned(),
                )
            })?,
        })
    }
}

fn read_command_row(row: &Row<'_>) -> rusqlite::Result<StoredVerificationCommandRow> {
    Ok(StoredVerificationCommandRow {
        name: row.get(0)?,
        executable: row.get(1)?,
        args_json: row.get(2)?,
        timeout_seconds: row.get(3)?,
        position: row.get(4)?,
    })
}

pub(crate) struct StoredVerificationFields {
    pub status: VerificationStatus,
    pub result: Option<VerificationResult>,
}

pub(crate) fn read_verification_fields(
    row: &Row<'_>,
    status_index: usize,
    json_index: usize,
    started_index: usize,
    finished_index: usize,
) -> rusqlite::Result<StoredVerificationFields> {
    let status: String = row.get(status_index)?;
    let json: Option<String> = row.get(json_index)?;
    let _started_at: Option<String> = row.get(started_index)?;
    let _finished_at: Option<String> = row.get(finished_index)?;
    let status = parse_verification_status(&status).map_err(|message| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                message,
            )),
        )
    })?;
    let result = json
        .map(|value| serde_json::from_str(&value))
        .transpose()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    Ok(StoredVerificationFields { status, result })
}

fn parse_verification_status(value: &str) -> Result<VerificationStatus, String> {
    match value {
        "not_required" => Ok(VerificationStatus::NotRequired),
        "running" => Ok(VerificationStatus::Running),
        "passed" => Ok(VerificationStatus::Passed),
        "failed" => Ok(VerificationStatus::Failed),
        "interrupted" => Ok(VerificationStatus::Interrupted),
        _ => Err(format!("unknown verification status: {value}")),
    }
}

pub(crate) fn verification_status_str(status: VerificationStatus) -> &'static str {
    match status {
        VerificationStatus::NotRequired => "not_required",
        VerificationStatus::Running => "running",
        VerificationStatus::Passed => "passed",
        VerificationStatus::Failed => "failed",
        VerificationStatus::Interrupted => "interrupted",
    }
}

pub(crate) fn report_id_for_work_item(
    connection: &rusqlite::Connection,
    work_item_id: &coldstart_core::WorkItemId,
) -> Result<Option<WorkReportId>, StorageError> {
    let value = connection
        .query_row(
            "SELECT id FROM work_reports WHERE work_item_id = ?1 AND verification_status = 'running' ORDER BY created_at DESC LIMIT 1",
            [work_item_id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    value
        .map(|value| {
            value.parse().map_err(|_| StorageError::InvalidId {
                kind: "work report",
                value,
            })
        })
        .transpose()
}
