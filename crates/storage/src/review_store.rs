//! Review 配置、规范修订和人工豁免的持久化。
//!
//! 配置与规范按修订保存，领取任务时冻结快照；Review 结果、失败原因和豁免都需
//! 保留可审计历史，不能通过覆盖旧行重写过去。

use std::collections::BTreeSet;

use chrono::{DateTime, Duration, Utc};
use coldstart_core::{
    ArtifactFormat, DomainEvent, DomainEventKind, LeaseId, PlanRevisionId, ProjectId,
    ReviewConfiguration, ReviewResult, ReviewScope, ReviewSnapshot, ReviewStandard,
    ReviewStandardId, ReviewStandardRevision, ReviewStandardRevisionId, ReviewStandardSource,
    ReviewStatus, ReviewValidationError, ReviewWaiver, VerificationResult, VerificationStatus,
    WorkItemId, WorkReportId, WorkReportRecord, WorkRunId, findings_hash, has_blocking_findings,
    sha256_hex,
};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, TransactionBehavior, params};

use crate::{
    Database, StorageError,
    database::{insert_event, parse_date, project_exists},
    plan_store::{query_report_by_id, refresh_work_items},
    verification_store::{record_work_item_project, update_verification_report},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewReviewStandardRevision {
    pub standard_id: ReviewStandardId,
    pub revision_id: ReviewStandardRevisionId,
    pub name: String,
    pub scope: ReviewScope,
    pub source: ReviewStandardSource,
    pub file_name: String,
    pub format: ArtifactFormat,
    pub content_hash: String,
    pub text_hash: String,
    pub character_count: usize,
    pub storage_relative_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingReview {
    pub report: WorkReportRecord,
    pub snapshot: ReviewSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeginReview {
    Started(PendingReview),
    Running(PendingReview),
    AlreadyResolved(WorkReportRecord),
}

impl Database {
    /// 保存 Review 模型配置与同意指纹，配置变化会要求重新确认。
    pub fn save_review_configuration(
        &mut self,
        configuration: &ReviewConfiguration,
    ) -> Result<ReviewConfiguration, StorageError> {
        configuration.validate().map_err(review_validation_error)?;
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !project_exists(&transaction, &configuration.project_id)? {
            return Err(StorageError::ProjectNotFound(configuration.project_id));
        }
        if let Some(credential_id) = configuration.credential_id {
            ensure_credential_project(&transaction, &configuration.project_id, credential_id)?;
        }
        transaction.execute(
            "INSERT INTO review_configurations (
                project_id, enabled, base_url, model, credential_id,
                blocking_severities_json, consent_fingerprint, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(project_id) DO UPDATE SET
                enabled = excluded.enabled, base_url = excluded.base_url,
                model = excluded.model, credential_id = excluded.credential_id,
                blocking_severities_json = excluded.blocking_severities_json,
                consent_fingerprint = excluded.consent_fingerprint,
                updated_at = excluded.updated_at",
            params![
                configuration.project_id.to_string(),
                i64::from(configuration.enabled),
                configuration.base_url,
                configuration.model,
                configuration.credential_id.map(|id| id.to_string()),
                serde_json::to_string(&configuration.blocking_severities)?,
                configuration.consent_fingerprint,
                configuration.updated_at.to_rfc3339(),
            ],
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                configuration.project_id,
                DomainEventKind::ProjectSettingUpdated {
                    setting_key: "review_configuration".to_owned(),
                },
            ),
        )?;
        transaction.commit()?;
        Ok(configuration.clone())
    }

    pub fn review_configuration(
        &self,
        project_id: &ProjectId,
    ) -> Result<Option<ReviewConfiguration>, StorageError> {
        let row = self
            .connection()
            .query_row(
                "SELECT project_id, enabled, base_url, model, credential_id,
                        blocking_severities_json, consent_fingerprint, updated_at
                 FROM review_configurations WHERE project_id = ?1",
                [project_id.to_string()],
                read_configuration_row,
            )
            .optional()?;
        row.map(StoredConfigurationRow::into_configuration)
            .transpose()
    }

    /// 创建代码规范修订，记录来源、范围、内容哈希与启用状态。
    pub fn create_review_standard_revision(
        &mut self,
        project_id: &ProjectId,
        input: NewReviewStandardRevision,
        now: DateTime<Utc>,
    ) -> Result<ReviewStandard, StorageError> {
        validate_standard_input(&input)?;
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !project_exists(&transaction, project_id)? {
            return Err(StorageError::ProjectNotFound(*project_id));
        }
        let (scope_kind, repository_key) = scope_columns(&input.scope);
        let existing = transaction
            .query_row(
                "SELECT deleted_at FROM review_standards WHERE id = ?1 AND project_id = ?2",
                params![input.standard_id.to_string(), project_id.to_string()],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?;
        let revision_number = transaction.query_row(
            "SELECT COALESCE(MAX(revision_number), 0) + 1
             FROM review_standard_revisions WHERE standard_id = ?1",
            [input.standard_id.to_string()],
            |row| row.get::<_, i64>(0),
        )?;
        match existing {
            Some(Some(_)) => return Err(StorageError::ReviewStandardDeleted(input.standard_id)),
            Some(None) => {
                transaction.execute(
                    "UPDATE review_standards SET name = ?1, scope_kind = ?2,
                        repository_key = ?3, current_revision_id = ?4, updated_at = ?5
                     WHERE id = ?6 AND project_id = ?7",
                    params![
                        input.name.trim(),
                        scope_kind,
                        repository_key,
                        input.revision_id.to_string(),
                        now.to_rfc3339(),
                        input.standard_id.to_string(),
                        project_id.to_string(),
                    ],
                )?;
            }
            None => {
                let position = transaction.query_row(
                    "SELECT COALESCE(MAX(position), -1) + 1
                     FROM review_standards WHERE project_id = ?1",
                    [project_id.to_string()],
                    |row| row.get::<_, i64>(0),
                )?;
                transaction.execute(
                    "INSERT INTO review_standards (
                        id, project_id, name, scope_kind, repository_key, enabled,
                        position, current_revision_id, deleted_at, created_at, updated_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7, NULL, ?8, ?8)",
                    params![
                        input.standard_id.to_string(),
                        project_id.to_string(),
                        input.name.trim(),
                        scope_kind,
                        repository_key,
                        position,
                        input.revision_id.to_string(),
                        now.to_rfc3339(),
                    ],
                )?;
            }
        }
        let character_count = i64::try_from(input.character_count)
            .map_err(|_| StorageError::ReviewInvalid("standard is too large".to_owned()))?;
        transaction.execute(
            "INSERT INTO review_standard_revisions (
                id, standard_id, project_id, revision_number, source, file_name,
                format, content_hash, text_hash, character_count,
                storage_relative_path, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                input.revision_id.to_string(),
                input.standard_id.to_string(),
                project_id.to_string(),
                revision_number,
                standard_source_str(input.source),
                input.file_name.trim(),
                input.format.to_string(),
                input.content_hash,
                input.text_hash,
                character_count,
                input.storage_relative_path,
                now.to_rfc3339(),
            ],
        )?;
        let standard = query_review_standard(&transaction, project_id, &input.standard_id)?
            .ok_or(StorageError::ReviewStandardNotFound(input.standard_id))?;
        transaction.commit()?;
        Ok(standard)
    }

    pub fn update_review_standard(
        &mut self,
        project_id: &ProjectId,
        standard_id: &ReviewStandardId,
        name: &str,
        scope: ReviewScope,
        now: DateTime<Utc>,
    ) -> Result<ReviewStandard, StorageError> {
        validate_standard_name(name)?;
        validate_scope(&scope)?;
        let (scope_kind, repository_key) = scope_columns(&scope);
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let updated = transaction.execute(
            "UPDATE review_standards SET name = ?1, scope_kind = ?2, repository_key = ?3,
                    updated_at = ?4
             WHERE id = ?5 AND project_id = ?6 AND deleted_at IS NULL",
            params![
                name.trim(),
                scope_kind,
                repository_key,
                now.to_rfc3339(),
                standard_id.to_string(),
                project_id.to_string(),
            ],
        )?;
        if updated == 0 {
            return Err(StorageError::ReviewStandardNotFound(*standard_id));
        }
        let standard = query_review_standard(&transaction, project_id, standard_id)?
            .ok_or(StorageError::ReviewStandardNotFound(*standard_id))?;
        transaction.commit()?;
        Ok(standard)
    }

    pub fn set_review_standard_enabled(
        &mut self,
        project_id: &ProjectId,
        standard_id: &ReviewStandardId,
        enabled: bool,
        now: DateTime<Utc>,
    ) -> Result<ReviewStandard, StorageError> {
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let updated = transaction.execute(
            "UPDATE review_standards SET enabled = ?1, updated_at = ?2
             WHERE id = ?3 AND project_id = ?4 AND deleted_at IS NULL",
            params![
                i64::from(enabled),
                now.to_rfc3339(),
                standard_id.to_string(),
                project_id.to_string(),
            ],
        )?;
        if updated == 0 {
            return Err(StorageError::ReviewStandardNotFound(*standard_id));
        }
        let standard = query_review_standard(&transaction, project_id, standard_id)?
            .ok_or(StorageError::ReviewStandardNotFound(*standard_id))?;
        transaction.commit()?;
        Ok(standard)
    }

    /// 原子重排规范顺序并校验集合完整性，避免顺序冲突。
    pub fn reorder_review_standards(
        &mut self,
        project_id: &ProjectId,
        ordered_ids: &[ReviewStandardId],
        now: DateTime<Utc>,
    ) -> Result<Vec<ReviewStandard>, StorageError> {
        let unique = ordered_ids.iter().copied().collect::<BTreeSet<_>>();
        if unique.len() != ordered_ids.len() {
            return Err(StorageError::ReviewInvalid(
                "review standard order contains duplicates".to_owned(),
            ));
        }
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = {
            let mut statement = transaction.prepare(
                "SELECT id FROM review_standards
                 WHERE project_id = ?1 AND deleted_at IS NULL",
            )?;
            let rows =
                statement.query_map([project_id.to_string()], |row| row.get::<_, String>(0))?;
            let mut ids = BTreeSet::new();
            for row in rows {
                let value = row?;
                ids.insert(value.parse::<ReviewStandardId>().map_err(|_| {
                    StorageError::InvalidId {
                        kind: "review standard",
                        value,
                    }
                })?);
            }
            ids
        };
        if existing != unique {
            return Err(StorageError::ReviewInvalid(
                "review standard order must include every active standard exactly once".to_owned(),
            ));
        }
        for (position, standard_id) in ordered_ids.iter().enumerate() {
            transaction.execute(
                "UPDATE review_standards SET position = ?1, updated_at = ?2
                 WHERE id = ?3 AND project_id = ?4 AND deleted_at IS NULL",
                params![
                    i64::try_from(position).unwrap_or(i64::MAX),
                    now.to_rfc3339(),
                    standard_id.to_string(),
                    project_id.to_string(),
                ],
            )?;
        }
        let standards = list_review_standards_in(&transaction, project_id)?;
        transaction.commit()?;
        Ok(standards)
    }

    pub fn soft_delete_review_standard(
        &mut self,
        project_id: &ProjectId,
        standard_id: &ReviewStandardId,
        now: DateTime<Utc>,
    ) -> Result<(), StorageError> {
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let updated = transaction.execute(
            "UPDATE review_standards SET deleted_at = ?1, enabled = 0, updated_at = ?1
             WHERE id = ?2 AND project_id = ?3 AND deleted_at IS NULL",
            params![
                now.to_rfc3339(),
                standard_id.to_string(),
                project_id.to_string()
            ],
        )?;
        if updated == 0 {
            return Err(StorageError::ReviewStandardNotFound(*standard_id));
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn list_review_standards(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<ReviewStandard>, StorageError> {
        list_review_standards_in(self.connection(), project_id)
    }

    pub fn review_standard_revision(
        &self,
        project_id: &ProjectId,
        revision_id: &ReviewStandardRevisionId,
    ) -> Result<Option<ReviewStandardRevision>, StorageError> {
        let row = self
            .connection()
            .query_row(
                "SELECT id, standard_id, project_id, revision_number, source, file_name,
                        format, content_hash, text_hash, character_count,
                        storage_relative_path, created_at
                 FROM review_standard_revisions WHERE project_id = ?1 AND id = ?2",
                params![project_id.to_string(), revision_id.to_string()],
                read_revision_row,
            )
            .optional()?;
        row.map(StoredRevisionRow::into_revision).transpose()
    }

    pub fn applicable_review_standards(
        &self,
        project_id: &ProjectId,
        repository_key: &str,
    ) -> Result<Vec<ReviewStandard>, StorageError> {
        let mut statement = self.connection().prepare(
            "SELECT s.id, s.project_id, s.name, s.scope_kind, s.repository_key,
                    s.enabled, s.position, s.deleted_at, s.created_at, s.updated_at,
                    r.id, r.revision_number, r.source, r.file_name, r.format,
                    r.content_hash, r.text_hash, r.character_count,
                    r.storage_relative_path, r.created_at
             FROM review_standards s
             JOIN review_standard_revisions r ON r.id = s.current_revision_id
             WHERE s.project_id = ?1 AND s.deleted_at IS NULL AND s.enabled = 1
               AND (s.scope_kind = 'project'
                    OR (s.scope_kind = 'repository' AND s.repository_key = ?2))
             ORDER BY CASE s.scope_kind WHEN 'project' THEN 0 ELSE 1 END,
                      s.position ASC, s.created_at ASC, s.id ASC",
        )?;
        let rows = statement.query_map(
            params![project_id.to_string(), repository_key],
            read_standard_row,
        )?;
        let mut standards = Vec::new();
        for row in rows {
            standards.push(row?.into_standard()?);
        }
        Ok(standards)
    }

    /// 在验证通过后启动 Review，冻结模型和规范快照并推进运行状态。
    pub fn begin_work_review(
        &mut self,
        project_id: &ProjectId,
        work_item_id: &WorkItemId,
        report_id: &WorkReportId,
        now: DateTime<Utc>,
    ) -> Result<BeginReview, StorageError> {
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = load_report_for_project(&transaction, project_id, work_item_id, report_id)?;
        match existing.review_status {
            ReviewStatus::Running => {
                let snapshot = snapshot_for_report(&transaction, &existing)?;
                transaction.commit()?;
                return Ok(BeginReview::Running(PendingReview {
                    report: existing,
                    snapshot,
                }));
            }
            ReviewStatus::NotRequired => {}
            _ => {
                transaction.commit()?;
                return Ok(BeginReview::AlreadyResolved(existing));
            }
        }
        if existing.verification_status != VerificationStatus::Passed {
            return Err(StorageError::ReviewInvalid(
                "work report verification must pass before Review".to_owned(),
            ));
        }
        let snapshot = snapshot_for_report(&transaction, &existing)?;
        start_review_in_transaction(&transaction, &existing, &snapshot, now)?;
        let report = query_report_by_id(&transaction, report_id)?
            .ok_or_else(|| StorageError::ReviewInvalid("work report disappeared".to_owned()))?;
        transaction.commit()?;
        Ok(BeginReview::Started(PendingReview { report, snapshot }))
    }

    /// 在同一事务中完成验证、启动 Review 并更新任务门禁。
    pub fn finish_work_verification_and_begin_review(
        &mut self,
        project_id: &ProjectId,
        work_item_id: &WorkItemId,
        report_id: &WorkReportId,
        verification: VerificationResult,
        now: DateTime<Utc>,
    ) -> Result<BeginReview, StorageError> {
        if verification.status != VerificationStatus::Passed {
            return Err(StorageError::VerificationCommandInvalid(
                "successful verification result must have passed status".to_owned(),
            ));
        }
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = load_report_for_project(&transaction, project_id, work_item_id, report_id)?;
        if existing.review_status == ReviewStatus::Running {
            let snapshot = snapshot_for_report(&transaction, &existing)?;
            transaction.commit()?;
            return Ok(BeginReview::Running(PendingReview {
                report: existing,
                snapshot,
            }));
        }
        if existing.review_status != ReviewStatus::NotRequired {
            transaction.commit()?;
            return Ok(BeginReview::AlreadyResolved(existing));
        }
        if existing.verification_status != VerificationStatus::Running {
            return Err(StorageError::ReviewInvalid(
                "work report is not running verification".to_owned(),
            ));
        }
        let snapshot = snapshot_for_report(&transaction, &existing)?;
        update_verification_report(&transaction, report_id, &verification, false, Vec::new())?;
        start_review_in_transaction(&transaction, &existing, &snapshot, now)?;
        let lease_id = report_lease_id(&transaction, &existing)?;
        let review_window = i64::try_from(coldstart_core::REVIEW_TIMEOUT_SECONDS)
            .unwrap_or(180)
            .saturating_add(600);
        let updated = transaction.execute(
            "UPDATE leases SET last_heartbeat_at = ?1,
                    expires_at = CASE WHEN expires_at > ?2 THEN expires_at ELSE ?2 END
             WHERE id = ?3 AND status = 'active' AND expires_at > ?1",
            params![
                now.to_rfc3339(),
                (now + Duration::seconds(review_window)).to_rfc3339(),
                lease_id.to_string(),
            ],
        )?;
        if updated == 0 {
            return Err(StorageError::LeaseExpired(*work_item_id));
        }
        insert_event(
            &transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::VerificationPassed {
                    work_item_id: *work_item_id,
                    report_id: *report_id,
                    snapshot_hash: verification.snapshot_hash,
                },
            ),
        )?;
        let report = query_report_by_id(&transaction, report_id)?
            .ok_or_else(|| StorageError::ReviewInvalid("work report disappeared".to_owned()))?;
        transaction.commit()?;
        Ok(BeginReview::Started(PendingReview { report, snapshot }))
    }

    /// 记录 Review 通过，消费租约并按结果完成或进入后续交付。
    pub fn finish_work_review_success(
        &mut self,
        project_id: &ProjectId,
        work_item_id: &WorkItemId,
        report_id: &WorkReportId,
        review: ReviewResult,
        now: DateTime<Utc>,
    ) -> Result<WorkReportRecord, StorageError> {
        if review.status != ReviewStatus::Passed {
            return Err(StorageError::ReviewInvalid(
                "successful Review result must have passed status".to_owned(),
            ));
        }
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = load_report_for_project(&transaction, project_id, work_item_id, report_id)?;
        if existing.review_status == ReviewStatus::Passed {
            transaction.commit()?;
            return Ok(existing);
        }
        if existing.review_status != ReviewStatus::Running {
            return Err(StorageError::ReviewInvalid(
                "work report is not running Review".to_owned(),
            ));
        }
        let snapshot = snapshot_for_report(&transaction, &existing)?;
        ensure_review_snapshot_hash(&review, &snapshot)?;
        if has_blocking_findings(
            &review.findings,
            &snapshot.configuration.blocking_severities,
        ) {
            return Err(StorageError::ReviewInvalid(
                "review contains blocking findings and cannot pass".to_owned(),
            ));
        }
        let (run_id, lease_id, plan_revision_id, task_key) =
            report_completion_scope(&transaction, project_id, work_item_id, &existing)?;
        transaction.execute(
            "UPDATE work_reports SET accepted = 1, rejection_reasons_json = '[]',
                    review_status = 'passed', review_json = ?1,
                    review_started_at = ?2, review_finished_at = ?3
             WHERE id = ?4",
            params![
                serde_json::to_string(&review)?,
                review.started_at.to_rfc3339(),
                review.finished_at.unwrap_or(now).to_rfc3339(),
                report_id.to_string(),
            ],
        )?;
        complete_reviewed_work(
            &transaction,
            project_id,
            work_item_id,
            &run_id,
            &lease_id,
            now,
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::ReviewPassed {
                    work_item_id: *work_item_id,
                    report_id: *report_id,
                    snapshot_hash: review.snapshot_hash.clone(),
                    finding_count: review.findings.len(),
                },
            ),
        )?;
        accept_reviewed_report(&transaction, *project_id, *work_item_id, *report_id)?;
        insert_work_completed_event(&transaction, *project_id, *work_item_id, &task_key)?;
        refresh_work_items(&transaction, project_id, &plan_revision_id, now)?;
        let record = query_report_by_id(&transaction, report_id)?
            .ok_or_else(|| StorageError::ReviewInvalid("work report disappeared".to_owned()))?;
        transaction.commit()?;
        Ok(record)
    }

    /// 记录模型失败并保持可重试或阻断状态，不误判为评审通过。
    pub fn finish_work_review_failure(
        &mut self,
        project_id: &ProjectId,
        work_item_id: &WorkItemId,
        report_id: &WorkReportId,
        review: ReviewResult,
        now: DateTime<Utc>,
        lease_duration: Duration,
    ) -> Result<WorkReportRecord, StorageError> {
        if !matches!(review.status, ReviewStatus::Blocked | ReviewStatus::Failed) {
            return Err(StorageError::ReviewInvalid(
                "failed Review result must have blocked or failed status".to_owned(),
            ));
        }
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = load_report_for_project(&transaction, project_id, work_item_id, report_id)?;
        if existing.review_status == review.status {
            transaction.commit()?;
            return Ok(existing);
        }
        if existing.review_status != ReviewStatus::Running {
            return Err(StorageError::ReviewInvalid(
                "work report is not running Review".to_owned(),
            ));
        }
        let snapshot = snapshot_for_report(&transaction, &existing)?;
        ensure_review_snapshot_hash(&review, &snapshot)?;
        let blocking_finding_count = review
            .findings
            .iter()
            .filter(|finding| {
                snapshot
                    .configuration
                    .blocking_severities
                    .contains(&finding.severity)
            })
            .count();
        if review.status == ReviewStatus::Blocked && blocking_finding_count == 0 {
            return Err(StorageError::ReviewInvalid(
                "blocked Review must contain a blocking finding".to_owned(),
            ));
        }
        let (run_id, lease_id) = report_run_and_lease(&transaction, &existing)?;
        let updated = transaction.execute(
            "UPDATE leases SET status = 'active', last_heartbeat_at = ?1, expires_at = ?2
             WHERE id = ?3 AND status = 'active' AND expires_at > ?1",
            params![
                now.to_rfc3339(),
                (now + lease_duration).to_rfc3339(),
                lease_id.to_string(),
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
        transaction.execute(
            "UPDATE work_reports SET accepted = 0, rejection_reasons_json = ?1,
                    review_status = ?2, review_json = ?3,
                    review_started_at = ?4, review_finished_at = ?5
             WHERE id = ?6",
            params![
                serde_json::to_string(&vec![review_rejection_reason(&review)])?,
                review.status.as_str(),
                serde_json::to_string(&review)?,
                review.started_at.to_rfc3339(),
                review.finished_at.unwrap_or(now).to_rfc3339(),
                report_id.to_string(),
            ],
        )?;
        let event = if review.status == ReviewStatus::Blocked {
            DomainEventKind::ReviewBlocked {
                work_item_id: *work_item_id,
                report_id: *report_id,
                snapshot_hash: review.snapshot_hash.clone(),
                blocking_finding_count,
            }
        } else {
            DomainEventKind::ReviewFailed {
                work_item_id: *work_item_id,
                report_id: *report_id,
                failure_code: review
                    .failure
                    .as_ref()
                    .map(|failure| failure.code.clone())
                    .unwrap_or_else(|| "review_failed".to_owned()),
            }
        };
        insert_event(&transaction, &DomainEvent::new(*project_id, event))?;
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
            .ok_or_else(|| StorageError::ReviewInvalid("work report disappeared".to_owned()))?;
        transaction.commit()?;
        Ok(record)
    }

    /// 对 blocked Review 应用一次人工豁免，并留下审计记录。
    pub fn waive_work_review(
        &mut self,
        project_id: &ProjectId,
        work_item_id: &WorkItemId,
        report_id: &WorkReportId,
        reason: &str,
        waived_by: &str,
        now: DateTime<Utc>,
    ) -> Result<WorkReportRecord, StorageError> {
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(StorageError::EmptyReviewWaiverReason);
        }
        if waived_by.trim().is_empty() {
            return Err(StorageError::ReviewInvalid(
                "Review waiver actor cannot be empty".to_owned(),
            ));
        }
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = load_report_for_project(&transaction, project_id, work_item_id, report_id)?;
        if existing.review_status == ReviewStatus::Waived {
            transaction.commit()?;
            return Ok(existing);
        }
        if existing.review_status != ReviewStatus::Blocked {
            return Err(StorageError::ReviewInvalid(
                "only a blocked Review can be waived".to_owned(),
            ));
        }
        let review = existing
            .review
            .clone()
            .ok_or_else(|| StorageError::ReviewInvalid("blocked Review is missing".to_owned()))?;
        let snapshot = snapshot_for_report(&transaction, &existing)?;
        let (run_id, lease_id, plan_revision_id, task_key) =
            report_completion_scope(&transaction, project_id, work_item_id, &existing)?;
        let waiver = ReviewWaiver {
            reason: reason.to_owned(),
            waived_by: waived_by.trim().to_owned(),
            waived_at: now,
            findings_hash: findings_hash(&review.findings),
            standards_hash: standards_hash(&snapshot)?,
        };
        transaction.execute(
            "UPDATE work_reports SET accepted = 1, rejection_reasons_json = '[]',
                    review_status = 'waived', review_waiver_json = ?1,
                    review_finished_at = ?2
             WHERE id = ?3",
            params![
                serde_json::to_string(&waiver)?,
                now.to_rfc3339(),
                report_id.to_string(),
            ],
        )?;
        complete_reviewed_work(
            &transaction,
            project_id,
            work_item_id,
            &run_id,
            &lease_id,
            now,
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::ReviewWaived {
                    work_item_id: *work_item_id,
                    report_id: *report_id,
                    waived_by: waiver.waived_by.clone(),
                },
            ),
        )?;
        accept_reviewed_report(&transaction, *project_id, *work_item_id, *report_id)?;
        insert_work_completed_event(&transaction, *project_id, *work_item_id, &task_key)?;
        refresh_work_items(&transaction, project_id, &plan_revision_id, now)?;
        let record = query_report_by_id(&transaction, report_id)?
            .ok_or_else(|| StorageError::ReviewInvalid("work report disappeared".to_owned()))?;
        transaction.commit()?;
        Ok(record)
    }
}

pub(crate) fn interrupt_running_review(
    transaction: &Transaction<'_>,
    work_item_id: &WorkItemId,
    now: DateTime<Utc>,
) -> Result<(), StorageError> {
    let row = transaction
        .query_row(
            "SELECT id, review_json, review_started_at
             FROM work_reports
             WHERE work_item_id = ?1 AND review_status = 'running'
             ORDER BY created_at DESC LIMIT 1",
            [work_item_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((report_id, review_json, review_started_at)) = row else {
        return Ok(());
    };
    let report_id: WorkReportId = report_id.parse().map_err(|_| StorageError::InvalidId {
        kind: "work report",
        value: report_id,
    })?;
    let mut review = review_json
        .map(|value| serde_json::from_str::<ReviewResult>(&value))
        .transpose()?
        .unwrap_or_else(|| ReviewResult {
            snapshot_hash: String::new(),
            status: ReviewStatus::Running,
            summary: "Review 正在执行".to_owned(),
            findings: Vec::new(),
            model: String::new(),
            failure: None,
            started_at: review_started_at
                .as_deref()
                .and_then(|value| parse_date(value).ok())
                .unwrap_or(now),
            finished_at: None,
            duration_ms: None,
        });
    review.status = ReviewStatus::Interrupted;
    review.finished_at = Some(now);
    review.duration_ms = Some((now - review.started_at).num_milliseconds().max(0) as u64);
    review.failure = Some(coldstart_core::ReviewFailure {
        code: "review_interrupted".to_owned(),
        message: "Review 因租约过期或程序中断而未完成".to_owned(),
    });
    transaction.execute(
        "UPDATE work_reports SET accepted = 0, review_status = 'interrupted',
                review_json = ?1, review_finished_at = ?2, rejection_reasons_json = ?3
         WHERE id = ?4",
        params![
            serde_json::to_string(&review)?,
            now.to_rfc3339(),
            serde_json::to_string(&vec!["Review 因租约过期或程序中断而未完成".to_owned()])?,
            report_id.to_string(),
        ],
    )?;
    insert_event(
        transaction,
        &DomainEvent::new(
            record_work_item_project(transaction, work_item_id)?,
            DomainEventKind::ReviewInterrupted {
                work_item_id: *work_item_id,
                report_id,
            },
        ),
    )?;
    Ok(())
}

pub(crate) fn review_configuration_in(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<Option<ReviewConfiguration>, StorageError> {
    let row = connection
        .query_row(
            "SELECT project_id, enabled, base_url, model, credential_id,
                    blocking_severities_json, consent_fingerprint, updated_at
             FROM review_configurations WHERE project_id = ?1",
            [project_id.to_string()],
            read_configuration_row,
        )
        .optional()?;
    row.map(StoredConfigurationRow::into_configuration)
        .transpose()
}

pub(crate) fn applicable_review_standards_in(
    connection: &Connection,
    project_id: &ProjectId,
    repository_key: &str,
) -> Result<Vec<ReviewStandard>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT s.id, s.project_id, s.name, s.scope_kind, s.repository_key,
                s.enabled, s.position, s.deleted_at, s.created_at, s.updated_at,
                r.id, r.revision_number, r.source, r.file_name, r.format,
                r.content_hash, r.text_hash, r.character_count,
                r.storage_relative_path, r.created_at
         FROM review_standards s
         JOIN review_standard_revisions r ON r.id = s.current_revision_id
         WHERE s.project_id = ?1 AND s.deleted_at IS NULL AND s.enabled = 1
           AND (s.scope_kind = 'project'
                OR (s.scope_kind = 'repository' AND s.repository_key = ?2))
         ORDER BY CASE s.scope_kind WHEN 'project' THEN 0 ELSE 1 END,
                  s.position ASC, s.created_at ASC, s.id ASC",
    )?;
    let rows = statement.query_map(
        params![project_id.to_string(), repository_key],
        read_standard_row,
    )?;
    let mut standards = Vec::new();
    for row in rows {
        standards.push(row?.into_standard()?);
    }
    Ok(standards)
}

fn start_review_in_transaction(
    transaction: &Transaction<'_>,
    report: &WorkReportRecord,
    snapshot: &ReviewSnapshot,
    now: DateTime<Utc>,
) -> Result<(), StorageError> {
    transaction.execute(
        "UPDATE work_reports SET review_status = 'running', review_json = NULL,
                review_started_at = ?1, review_finished_at = NULL,
                review_waiver_json = NULL, accepted = 0
         WHERE id = ?2",
        params![now.to_rfc3339(), report.id.to_string()],
    )?;
    let (run_id, _) = report_run_and_lease(transaction, report)?;
    transaction.execute(
        "UPDATE work_runs SET status = 'running', ended_at = NULL WHERE id = ?1",
        [run_id.to_string()],
    )?;
    transaction.execute(
        "UPDATE work_items SET status = 'in_progress', updated_at = ?1 WHERE id = ?2",
        params![now.to_rfc3339(), report.work_item_id.to_string()],
    )?;
    insert_event(
        transaction,
        &DomainEvent::new(
            record_work_item_project(transaction, &report.work_item_id)?,
            DomainEventKind::ReviewStarted {
                work_item_id: report.work_item_id,
                report_id: report.id,
                snapshot_hash: snapshot.content_hash(),
            },
        ),
    )?;
    Ok(())
}

fn complete_reviewed_work(
    transaction: &Transaction<'_>,
    project_id: &ProjectId,
    work_item_id: &WorkItemId,
    run_id: &WorkRunId,
    lease_id: &LeaseId,
    now: DateTime<Utc>,
) -> Result<(), StorageError> {
    let consumed = transaction.execute(
        "UPDATE leases SET status = 'consumed', released_at = ?1
         WHERE id = ?2 AND status = 'active' AND expires_at > ?1",
        params![now.to_rfc3339(), lease_id.to_string()],
    )?;
    if consumed == 0 {
        return Err(StorageError::LeaseExpired(*work_item_id));
    }
    transaction.execute(
        "UPDATE work_runs SET status = 'completed', ended_at = ?1 WHERE id = ?2",
        params![now.to_rfc3339(), run_id.to_string()],
    )?;
    transaction.execute(
        "UPDATE work_items SET status = 'done', updated_at = ?1, completed_at = ?1
         WHERE id = ?2",
        params![now.to_rfc3339(), work_item_id.to_string()],
    )?;
    let _ = project_id;
    Ok(())
}

fn accept_reviewed_report(
    transaction: &Transaction<'_>,
    project_id: ProjectId,
    work_item_id: WorkItemId,
    report_id: WorkReportId,
) -> Result<(), StorageError> {
    insert_event(
        transaction,
        &DomainEvent::new(
            project_id,
            DomainEventKind::WorkReportSubmitted {
                work_item_id,
                report_id,
                accepted: true,
            },
        ),
    )
}

fn insert_work_completed_event(
    transaction: &Transaction<'_>,
    project_id: ProjectId,
    work_item_id: WorkItemId,
    task_key: &str,
) -> Result<(), StorageError> {
    insert_event(
        transaction,
        &DomainEvent::new(
            project_id,
            DomainEventKind::WorkCompleted {
                work_item_id,
                task_key: task_key.to_owned(),
            },
        ),
    )
}

fn load_report_for_project(
    transaction: &Transaction<'_>,
    project_id: &ProjectId,
    work_item_id: &WorkItemId,
    report_id: &WorkReportId,
) -> Result<WorkReportRecord, StorageError> {
    let report = query_report_by_id(transaction, report_id)?
        .ok_or_else(|| StorageError::ReviewInvalid("work report does not exist".to_owned()))?;
    let matches: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM work_items WHERE id = ?1 AND project_id = ?2)",
        params![work_item_id.to_string(), project_id.to_string()],
        |row| row.get(0),
    )?;
    if report.work_item_id != *work_item_id || !matches {
        return Err(StorageError::ReviewInvalid(
            "work report does not belong to the requested project and work item".to_owned(),
        ));
    }
    Ok(report)
}

fn snapshot_for_report(
    transaction: &Transaction<'_>,
    report: &WorkReportRecord,
) -> Result<ReviewSnapshot, StorageError> {
    let row = transaction
        .query_row(
            "SELECT review_snapshot_json, review_snapshot_hash FROM work_runs WHERE id = ?1",
            [report.work_run_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            },
        )
        .optional()?;
    let (snapshot_json, snapshot_hash) =
        row.ok_or(StorageError::ReviewNotConfigured(report.work_item_id))?;
    let snapshot_json =
        snapshot_json.ok_or(StorageError::ReviewNotConfigured(report.work_item_id))?;
    let snapshot: ReviewSnapshot = serde_json::from_str(&snapshot_json)?;
    snapshot.validate().map_err(review_validation_error)?;
    if snapshot_hash.as_deref() != Some(snapshot.content_hash().as_str()) {
        return Err(StorageError::ReviewInvalid(
            "Review snapshot hash does not match the persisted snapshot".to_owned(),
        ));
    }
    Ok(snapshot)
}

fn report_run_and_lease(
    transaction: &Transaction<'_>,
    report: &WorkReportRecord,
) -> Result<(WorkRunId, LeaseId), StorageError> {
    let (run_id, lease_id): (String, String) = transaction.query_row(
        "SELECT id, lease_id FROM work_runs WHERE id = ?1",
        [report.work_run_id.to_string()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok((
        run_id.parse().map_err(|_| StorageError::InvalidId {
            kind: "work run",
            value: run_id,
        })?,
        lease_id.parse().map_err(|_| StorageError::InvalidId {
            kind: "lease",
            value: lease_id,
        })?,
    ))
}

fn report_lease_id(
    transaction: &Transaction<'_>,
    report: &WorkReportRecord,
) -> Result<LeaseId, StorageError> {
    report_run_and_lease(transaction, report).map(|(_, lease_id)| lease_id)
}

fn report_completion_scope(
    transaction: &Transaction<'_>,
    project_id: &ProjectId,
    work_item_id: &WorkItemId,
    report: &WorkReportRecord,
) -> Result<(WorkRunId, LeaseId, PlanRevisionId, String), StorageError> {
    let row = transaction.query_row(
        "SELECT r.id, r.lease_id, w.plan_revision_id, w.task_key
         FROM work_runs r JOIN work_items w ON w.id = r.work_item_id
         WHERE r.id = ?1 AND w.id = ?2 AND w.project_id = ?3",
        params![
            report.work_run_id.to_string(),
            work_item_id.to_string(),
            project_id.to_string()
        ],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    )?;
    Ok((
        row.0.parse().map_err(|_| StorageError::InvalidId {
            kind: "work run",
            value: row.0,
        })?,
        row.1.parse().map_err(|_| StorageError::InvalidId {
            kind: "lease",
            value: row.1,
        })?,
        row.2.parse().map_err(|_| StorageError::InvalidId {
            kind: "plan revision",
            value: row.2,
        })?,
        row.3,
    ))
}

fn ensure_review_snapshot_hash(
    review: &ReviewResult,
    snapshot: &ReviewSnapshot,
) -> Result<(), StorageError> {
    if review.snapshot_hash != snapshot.content_hash() {
        return Err(StorageError::ReviewInvalid(
            "Review result snapshot hash does not match the claim snapshot".to_owned(),
        ));
    }
    Ok(())
}

fn review_rejection_reason(review: &ReviewResult) -> String {
    let detail = review
        .failure
        .as_ref()
        .map(|failure| format!("{}: {}", failure.code, failure.message))
        .unwrap_or_else(|| review.summary.clone());
    let detail = detail.chars().take(1_500).collect::<String>();
    format!("AI Review {}：{}", review.status.as_str(), detail)
}

fn standards_hash(snapshot: &ReviewSnapshot) -> Result<String, StorageError> {
    Ok(sha256_hex(&serde_json::to_vec(&snapshot.standards)?))
}

fn validate_standard_input(input: &NewReviewStandardRevision) -> Result<(), StorageError> {
    validate_standard_name(&input.name)?;
    validate_scope(&input.scope)?;
    if input.file_name.trim().is_empty()
        || input.content_hash.trim().is_empty()
        || input.text_hash.trim().is_empty()
        || input.storage_relative_path.trim().is_empty()
    {
        return Err(StorageError::ReviewInvalid(
            "review standard revision metadata cannot be empty".to_owned(),
        ));
    }
    if input.character_count > coldstart_core::MAX_REVIEW_STANDARD_CHARS {
        return Err(StorageError::ReviewInvalid(format!(
            "review standard exceeds {} characters",
            coldstart_core::MAX_REVIEW_STANDARD_CHARS
        )));
    }
    Ok(())
}

fn validate_standard_name(name: &str) -> Result<(), StorageError> {
    if name.trim().is_empty() {
        return Err(StorageError::ReviewInvalid(
            "review standard name cannot be empty".to_owned(),
        ));
    }
    Ok(())
}

fn validate_scope(scope: &ReviewScope) -> Result<(), StorageError> {
    if let ReviewScope::Repository(repository_key) = scope
        && !coldstart_core::REPOSITORY_KEYS.contains(&repository_key.as_str())
    {
        return Err(StorageError::ReviewInvalid(format!(
            "invalid repository key: {repository_key}"
        )));
    }
    Ok(())
}

fn scope_columns(scope: &ReviewScope) -> (&'static str, Option<&str>) {
    match scope {
        ReviewScope::Project => ("project", None),
        ReviewScope::Repository(repository_key) => ("repository", Some(repository_key)),
    }
}

fn standard_source_str(source: ReviewStandardSource) -> &'static str {
    match source {
        ReviewStandardSource::Upload => "upload",
        ReviewStandardSource::Manual => "manual",
    }
}

fn parse_standard_source(value: &str) -> Option<ReviewStandardSource> {
    match value {
        "upload" => Some(ReviewStandardSource::Upload),
        "manual" => Some(ReviewStandardSource::Manual),
        _ => None,
    }
}

fn parse_scope(scope_kind: &str, repository_key: Option<String>) -> Option<ReviewScope> {
    match scope_kind {
        "project" => Some(ReviewScope::Project),
        "repository" => repository_key.map(ReviewScope::Repository),
        _ => None,
    }
}

fn review_validation_error(error: ReviewValidationError) -> StorageError {
    StorageError::ReviewInvalid(error.to_string())
}

fn ensure_credential_project(
    transaction: &Transaction<'_>,
    project_id: &ProjectId,
    credential_id: coldstart_core::CredentialId,
) -> Result<(), StorageError> {
    let exists: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM credentials WHERE id = ?1 AND project_id = ?2)",
        params![credential_id.to_string(), project_id.to_string()],
        |row| row.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(StorageError::CredentialProjectMismatch(credential_id))
    }
}

fn list_review_standards_in(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<Vec<ReviewStandard>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT s.id, s.project_id, s.name, s.scope_kind, s.repository_key,
                s.enabled, s.position, s.deleted_at, s.created_at, s.updated_at,
                r.id, r.revision_number, r.source, r.file_name, r.format,
                r.content_hash, r.text_hash, r.character_count,
                r.storage_relative_path, r.created_at
         FROM review_standards s
         JOIN review_standard_revisions r ON r.id = s.current_revision_id
         WHERE s.project_id = ?1 AND s.deleted_at IS NULL
         ORDER BY CASE s.scope_kind WHEN 'project' THEN 0 ELSE 1 END,
                  s.position ASC, s.created_at ASC, s.id ASC",
    )?;
    let rows = statement.query_map([project_id.to_string()], read_standard_row)?;
    let mut standards = Vec::new();
    for row in rows {
        standards.push(row?.into_standard()?);
    }
    Ok(standards)
}

fn query_review_standard(
    connection: &Connection,
    project_id: &ProjectId,
    standard_id: &ReviewStandardId,
) -> Result<Option<ReviewStandard>, StorageError> {
    let row = connection
        .query_row(
            "SELECT s.id, s.project_id, s.name, s.scope_kind, s.repository_key,
                    s.enabled, s.position, s.deleted_at, s.created_at, s.updated_at,
                    r.id, r.revision_number, r.source, r.file_name, r.format,
                    r.content_hash, r.text_hash, r.character_count,
                    r.storage_relative_path, r.created_at
             FROM review_standards s
             JOIN review_standard_revisions r ON r.id = s.current_revision_id
             WHERE s.project_id = ?1 AND s.id = ?2 AND s.deleted_at IS NULL",
            params![project_id.to_string(), standard_id.to_string()],
            read_standard_row,
        )
        .optional()?;
    row.map(StoredStandardRow::into_standard).transpose()
}

struct StoredConfigurationRow {
    project_id: String,
    enabled: i64,
    base_url: String,
    model: String,
    credential_id: Option<String>,
    blocking_severities_json: String,
    consent_fingerprint: Option<String>,
    updated_at: String,
}

impl StoredConfigurationRow {
    fn into_configuration(self) -> Result<ReviewConfiguration, StorageError> {
        Ok(ReviewConfiguration {
            project_id: self
                .project_id
                .parse()
                .map_err(|_| StorageError::InvalidId {
                    kind: "project",
                    value: self.project_id,
                })?,
            enabled: self.enabled != 0,
            base_url: self.base_url,
            model: self.model,
            credential_id: self
                .credential_id
                .map(|value| {
                    value.parse().map_err(|_| StorageError::InvalidId {
                        kind: "credential",
                        value,
                    })
                })
                .transpose()?,
            blocking_severities: serde_json::from_str(&self.blocking_severities_json)?,
            consent_fingerprint: self.consent_fingerprint,
            updated_at: parse_date(&self.updated_at)?,
        })
    }
}

fn read_configuration_row(row: &Row<'_>) -> rusqlite::Result<StoredConfigurationRow> {
    Ok(StoredConfigurationRow {
        project_id: row.get(0)?,
        enabled: row.get(1)?,
        base_url: row.get(2)?,
        model: row.get(3)?,
        credential_id: row.get(4)?,
        blocking_severities_json: row.get(5)?,
        consent_fingerprint: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

struct StoredStandardRow {
    id: String,
    project_id: String,
    name: String,
    scope_kind: String,
    repository_key: Option<String>,
    enabled: i64,
    position: i64,
    deleted_at: Option<String>,
    created_at: String,
    updated_at: String,
    revision_id: String,
    revision_number: i64,
    source: String,
    file_name: String,
    format: String,
    content_hash: String,
    text_hash: String,
    character_count: i64,
    storage_relative_path: String,
    revision_created_at: String,
}

impl StoredStandardRow {
    fn into_standard(self) -> Result<ReviewStandard, StorageError> {
        let project_id = self
            .project_id
            .parse()
            .map_err(|_| StorageError::InvalidId {
                kind: "project",
                value: self.project_id.clone(),
            })?;
        let standard_id = self.id.parse().map_err(|_| StorageError::InvalidId {
            kind: "review standard",
            value: self.id,
        })?;
        let revision_id = self
            .revision_id
            .parse()
            .map_err(|_| StorageError::InvalidId {
                kind: "review standard revision",
                value: self.revision_id,
            })?;
        Ok(ReviewStandard {
            id: standard_id,
            project_id,
            name: self.name,
            scope: parse_scope(&self.scope_kind, self.repository_key).ok_or_else(|| {
                StorageError::ReviewInvalid("stored review scope is invalid".to_owned())
            })?,
            enabled: self.enabled != 0,
            position: u32::try_from(self.position).map_err(|_| {
                StorageError::ReviewInvalid("stored position is invalid".to_owned())
            })?,
            current_revision: ReviewStandardRevision {
                id: revision_id,
                standard_id,
                project_id,
                revision_number: u64::try_from(self.revision_number).map_err(|_| {
                    StorageError::ReviewInvalid("stored revision number is invalid".to_owned())
                })?,
                source: parse_standard_source(&self.source).ok_or_else(|| {
                    StorageError::ReviewInvalid("stored standard source is invalid".to_owned())
                })?,
                file_name: self.file_name,
                format: self.format.parse().map_err(|_| {
                    StorageError::ReviewInvalid("stored standard format is invalid".to_owned())
                })?,
                content_hash: self.content_hash,
                text_hash: self.text_hash,
                character_count: usize::try_from(self.character_count).map_err(|_| {
                    StorageError::ReviewInvalid("stored character count is invalid".to_owned())
                })?,
                storage_relative_path: self.storage_relative_path,
                created_at: parse_date(&self.revision_created_at)?,
            },
            deleted_at: self.deleted_at.as_deref().map(parse_date).transpose()?,
            created_at: parse_date(&self.created_at)?,
            updated_at: parse_date(&self.updated_at)?,
        })
    }
}

fn read_standard_row(row: &Row<'_>) -> rusqlite::Result<StoredStandardRow> {
    Ok(StoredStandardRow {
        id: row.get(0)?,
        project_id: row.get(1)?,
        name: row.get(2)?,
        scope_kind: row.get(3)?,
        repository_key: row.get(4)?,
        enabled: row.get(5)?,
        position: row.get(6)?,
        deleted_at: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        revision_id: row.get(10)?,
        revision_number: row.get(11)?,
        source: row.get(12)?,
        file_name: row.get(13)?,
        format: row.get(14)?,
        content_hash: row.get(15)?,
        text_hash: row.get(16)?,
        character_count: row.get(17)?,
        storage_relative_path: row.get(18)?,
        revision_created_at: row.get(19)?,
    })
}

struct StoredRevisionRow {
    id: String,
    standard_id: String,
    project_id: String,
    revision_number: i64,
    source: String,
    file_name: String,
    format: String,
    content_hash: String,
    text_hash: String,
    character_count: i64,
    storage_relative_path: String,
    created_at: String,
}

impl StoredRevisionRow {
    fn into_revision(self) -> Result<ReviewStandardRevision, StorageError> {
        Ok(ReviewStandardRevision {
            id: self.id.parse().map_err(|_| StorageError::InvalidId {
                kind: "review standard revision",
                value: self.id,
            })?,
            standard_id: self
                .standard_id
                .parse()
                .map_err(|_| StorageError::InvalidId {
                    kind: "review standard",
                    value: self.standard_id,
                })?,
            project_id: self
                .project_id
                .parse()
                .map_err(|_| StorageError::InvalidId {
                    kind: "project",
                    value: self.project_id,
                })?,
            revision_number: u64::try_from(self.revision_number).map_err(|_| {
                StorageError::ReviewInvalid("stored revision number is invalid".to_owned())
            })?,
            source: parse_standard_source(&self.source).ok_or_else(|| {
                StorageError::ReviewInvalid("stored standard source is invalid".to_owned())
            })?,
            file_name: self.file_name,
            format: self.format.parse().map_err(|_| {
                StorageError::ReviewInvalid("stored standard format is invalid".to_owned())
            })?,
            content_hash: self.content_hash,
            text_hash: self.text_hash,
            character_count: usize::try_from(self.character_count).map_err(|_| {
                StorageError::ReviewInvalid("stored character count is invalid".to_owned())
            })?,
            storage_relative_path: self.storage_relative_path,
            created_at: parse_date(&self.created_at)?,
        })
    }
}

fn read_revision_row(row: &Row<'_>) -> rusqlite::Result<StoredRevisionRow> {
    Ok(StoredRevisionRow {
        id: row.get(0)?,
        standard_id: row.get(1)?,
        project_id: row.get(2)?,
        revision_number: row.get(3)?,
        source: row.get(4)?,
        file_name: row.get(5)?,
        format: row.get(6)?,
        content_hash: row.get(7)?,
        text_hash: row.get(8)?,
        character_count: row.get(9)?,
        storage_relative_path: row.get(10)?,
        created_at: row.get(11)?,
    })
}
