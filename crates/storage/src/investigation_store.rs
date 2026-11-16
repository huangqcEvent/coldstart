//! 历史代码调查的持久化与领取状态机。
//!
//! 调查报告只保存结构化定位证据，不保存源码正文或 Secret。领取令牌以哈希形式
//! 落库，提交过程在 SQLite 立即事务内校验租约与幂等边界。

use std::str::FromStr;

use chrono::{DateTime, Duration, Utc};
use coldstart_core::{
    CODE_INVESTIGATION_LEASE_MINUTES, CodeFinding, CodeInvestigation, CodeInvestigationClaim,
    CodeInvestigationId, CodeInvestigationReport, CodeInvestigationStatus, DomainEvent,
    DomainEventKind, ProjectId, RepositoryId,
};
use rusqlite::{OptionalExtension, Row, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    Database, StorageError,
    database::{insert_event, parse_date, project_exists},
};

impl Database {
    /// 创建一条待 Codex 领取的只读调查请求。
    pub fn create_code_investigation(
        &mut self,
        project_id: &ProjectId,
        repository_key: &str,
        base_ref: &str,
        question: &str,
        now: DateTime<Utc>,
    ) -> Result<CodeInvestigation, StorageError> {
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !project_exists(&transaction, project_id)? {
            return Err(StorageError::ProjectNotFound(*project_id));
        }
        let repository = transaction
            .query_row(
                "SELECT id FROM repositories WHERE project_id = ?1 AND repository_key = ?2",
                params![project_id.to_string(), repository_key],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| StorageError::RepositoryNotFound(repository_key.to_owned()))?;
        let investigation = CodeInvestigation {
            id: CodeInvestigationId::new(),
            project_id: *project_id,
            repository_id: parse_id(&repository, "repository")?,
            repository_key: repository_key.to_owned(),
            base_ref: base_ref.to_owned(),
            question: question.trim().to_owned(),
            status: CodeInvestigationStatus::Requested,
            agent_profile: None,
            lease_expires_at: None,
            commit_sha: None,
            summary: None,
            findings: Vec::new(),
            limitations: Vec::new(),
            report_hash: None,
            rejection_reason: None,
            created_at: now,
            claimed_at: None,
            submitted_at: None,
            confirmed_at: None,
        };
        transaction.execute(
            "INSERT INTO code_investigations (
                id, project_id, repository_id, repository_key, base_ref, question, status,
                findings_json, limitations_json, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'requested', '[]', '[]', ?7)",
            params![
                investigation.id.to_string(),
                project_id.to_string(),
                investigation.repository_id.to_string(),
                investigation.repository_key,
                investigation.base_ref,
                investigation.question,
                now.to_rfc3339(),
            ],
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::CodeInvestigationCreated {
                    investigation_id: investigation.id,
                    repository_key: investigation.repository_key.clone(),
                    base_ref: investigation.base_ref.clone(),
                },
            ),
        )?;
        transaction.commit()?;
        Ok(investigation)
    }

    pub fn list_code_investigations(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<CodeInvestigation>, StorageError> {
        let mut statement = self.connection().prepare(
            "SELECT id, project_id, repository_id, repository_key, base_ref, question, status,
                    agent_profile, lease_expires_at, commit_sha, summary, findings_json,
                    limitations_json, report_hash, rejection_reason, created_at, claimed_at,
                    submitted_at, confirmed_at
             FROM code_investigations WHERE project_id = ?1
             ORDER BY CASE status
                 WHEN 'submitted' THEN 0
                 WHEN 'claimed' THEN 1
                 WHEN 'requested' THEN 2
                 ELSE 3 END,
                 created_at DESC, id ASC",
        )?;
        let rows = statement.query_map([project_id.to_string()], read_investigation_row)?;
        let mut investigations = Vec::new();
        for row in rows {
            investigations.push(row?.into_investigation()?);
        }
        Ok(investigations)
    }

    /// 原子领取最早的待调查项；过期领取会自动回到队列，允许下一个 Agent 接手。
    pub fn claim_next_code_investigation(
        &mut self,
        project_id: &ProjectId,
        repository_key: Option<&str>,
        agent_profile: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<CodeInvestigationClaim>, StorageError> {
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !project_exists(&transaction, project_id)? {
            return Err(StorageError::ProjectNotFound(*project_id));
        }
        transaction.execute(
            "UPDATE code_investigations
             SET status = 'requested', agent_profile = NULL, claim_token_hash = NULL,
                 lease_expires_at = NULL, claimed_at = NULL
             WHERE project_id = ?1 AND status = 'claimed' AND lease_expires_at < ?2",
            params![project_id.to_string(), now.to_rfc3339()],
        )?;
        let candidate = transaction
            .query_row(
                "SELECT id FROM code_investigations
                 WHERE project_id = ?1 AND status = 'requested'
                   AND (?2 IS NULL OR repository_key = ?2)
                 ORDER BY created_at ASC, id ASC LIMIT 1",
                params![project_id.to_string(), repository_key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(candidate) = candidate else {
            transaction.commit()?;
            return Ok(None);
        };
        let id = parse_id::<CodeInvestigationId>(&candidate, "code investigation")?;
        let claim_token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let expires_at = now + Duration::minutes(CODE_INVESTIGATION_LEASE_MINUTES);
        transaction.execute(
            "UPDATE code_investigations
             SET status = 'claimed', agent_profile = ?1, claim_token_hash = ?2,
                 lease_expires_at = ?3, claimed_at = ?4
             WHERE id = ?5 AND status = 'requested'",
            params![
                agent_profile,
                sha256_hex(claim_token.as_bytes()),
                expires_at.to_rfc3339(),
                now.to_rfc3339(),
                id.to_string(),
            ],
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::CodeInvestigationClaimed {
                    investigation_id: id,
                    agent_profile: agent_profile.to_owned(),
                },
            ),
        )?;
        let investigation = load_investigation(&transaction, project_id, id)?;
        transaction.commit()?;
        Ok(Some(CodeInvestigationClaim {
            investigation,
            claim_token,
        }))
    }

    pub fn submit_code_investigation(
        &mut self,
        project_id: &ProjectId,
        investigation_id: CodeInvestigationId,
        claim_token: &str,
        report: CodeInvestigationReport,
        now: DateTime<Utc>,
    ) -> Result<CodeInvestigation, StorageError> {
        report
            .validate()
            .map_err(|error| StorageError::ReportRejected(error.to_string()))?;
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut investigation = load_investigation(&transaction, project_id, investigation_id)?;
        let token_hash = sha256_hex(claim_token.as_bytes());
        let stored: Option<(Option<String>, Option<String>, Option<String>)> = transaction
            .query_row(
                "SELECT claim_token_hash, report_hash, report_idempotency_key
                 FROM code_investigations WHERE id = ?1",
                [investigation_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let (stored_hash, stored_report_hash, stored_idempotency_key) =
            stored.ok_or(StorageError::CodeInvestigationNotFound(investigation_id))?;
        if stored_hash.as_deref() != Some(token_hash.as_str()) {
            return Err(StorageError::CodeInvestigationClaimInvalid(
                investigation_id,
            ));
        }
        if investigation.status == CodeInvestigationStatus::Submitted {
            if stored_report_hash.as_deref() != Some(report.content_hash().as_str())
                || stored_idempotency_key.as_deref() != Some(report.idempotency_key.as_str())
            {
                return Err(StorageError::CodeInvestigationReportConflict(
                    investigation_id,
                ));
            }
            transaction.commit()?;
            return Ok(investigation);
        }
        if investigation.status != CodeInvestigationStatus::Claimed {
            return Err(StorageError::CodeInvestigationNotClaimable(
                investigation_id,
            ));
        }
        if investigation
            .lease_expires_at
            .is_none_or(|expires_at| expires_at < now)
        {
            return Err(StorageError::CodeInvestigationLeaseExpired(
                investigation_id,
            ));
        }
        let findings_json = serde_json::to_string(&report.findings)?;
        let limitations_json = serde_json::to_string(&report.limitations)?;
        let report_hash = report.content_hash();
        transaction.execute(
            "UPDATE code_investigations
             SET status = 'submitted', commit_sha = ?1, summary = ?2, findings_json = ?3,
                 limitations_json = ?4, report_hash = ?5, report_idempotency_key = ?6,
                 submitted_at = ?7
             WHERE id = ?8",
            params![
                report.commit_sha,
                report.summary,
                findings_json,
                limitations_json,
                report_hash,
                report.idempotency_key,
                now.to_rfc3339(),
                investigation_id.to_string(),
            ],
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::CodeInvestigationSubmitted {
                    investigation_id,
                    commit_sha: report.commit_sha,
                    finding_count: report.findings.len(),
                },
            ),
        )?;
        investigation = load_investigation(&transaction, project_id, investigation_id)?;
        transaction.commit()?;
        Ok(investigation)
    }

    pub fn confirm_code_investigation(
        &mut self,
        project_id: &ProjectId,
        investigation_id: CodeInvestigationId,
        now: DateTime<Utc>,
    ) -> Result<CodeInvestigation, StorageError> {
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let investigation = load_investigation(&transaction, project_id, investigation_id)?;
        if investigation.status != CodeInvestigationStatus::Submitted {
            return Err(StorageError::CodeInvestigationNotSubmitted(
                investigation_id,
            ));
        }
        transaction.execute(
            "UPDATE code_investigations SET status = 'confirmed', confirmed_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), investigation_id.to_string()],
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::CodeInvestigationConfirmed { investigation_id },
            ),
        )?;
        let investigation = load_investigation(&transaction, project_id, investigation_id)?;
        transaction.commit()?;
        Ok(investigation)
    }

    pub fn reject_code_investigation(
        &mut self,
        project_id: &ProjectId,
        investigation_id: CodeInvestigationId,
        reason: &str,
    ) -> Result<CodeInvestigation, StorageError> {
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(StorageError::EmptyCodeInvestigationRejection);
        }
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let investigation = load_investigation(&transaction, project_id, investigation_id)?;
        if investigation.status != CodeInvestigationStatus::Submitted {
            return Err(StorageError::CodeInvestigationNotSubmitted(
                investigation_id,
            ));
        }
        transaction.execute(
            "UPDATE code_investigations SET status = 'rejected', rejection_reason = ?1
             WHERE id = ?2",
            params![reason, investigation_id.to_string()],
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::CodeInvestigationRejected { investigation_id },
            ),
        )?;
        let investigation = load_investigation(&transaction, project_id, investigation_id)?;
        transaction.commit()?;
        Ok(investigation)
    }
}

fn load_investigation(
    transaction: &rusqlite::Transaction<'_>,
    project_id: &ProjectId,
    investigation_id: CodeInvestigationId,
) -> Result<CodeInvestigation, StorageError> {
    let row = transaction
        .query_row(
            "SELECT id, project_id, repository_id, repository_key, base_ref, question, status,
                    agent_profile, lease_expires_at, commit_sha, summary, findings_json,
                    limitations_json, report_hash, rejection_reason, created_at, claimed_at,
                    submitted_at, confirmed_at
             FROM code_investigations WHERE id = ?1 AND project_id = ?2",
            params![investigation_id.to_string(), project_id.to_string()],
            read_investigation_row,
        )
        .optional()?
        .ok_or(StorageError::CodeInvestigationNotFound(investigation_id))?;
    row.into_investigation()
}

struct StoredInvestigationRow {
    id: String,
    project_id: String,
    repository_id: String,
    repository_key: String,
    base_ref: String,
    question: String,
    status: String,
    agent_profile: Option<String>,
    lease_expires_at: Option<String>,
    commit_sha: Option<String>,
    summary: Option<String>,
    findings_json: String,
    limitations_json: String,
    report_hash: Option<String>,
    rejection_reason: Option<String>,
    created_at: String,
    claimed_at: Option<String>,
    submitted_at: Option<String>,
    confirmed_at: Option<String>,
}

impl StoredInvestigationRow {
    fn into_investigation(self) -> Result<CodeInvestigation, StorageError> {
        Ok(CodeInvestigation {
            id: parse_id(&self.id, "code investigation")?,
            project_id: parse_id(&self.project_id, "project")?,
            repository_id: parse_id::<RepositoryId>(&self.repository_id, "repository")?,
            repository_key: self.repository_key,
            base_ref: self.base_ref,
            question: self.question,
            status: parse_status(&self.status)?,
            agent_profile: self.agent_profile,
            lease_expires_at: parse_optional_date(self.lease_expires_at)?,
            commit_sha: self.commit_sha,
            summary: self.summary,
            findings: serde_json::from_str::<Vec<CodeFinding>>(&self.findings_json)?,
            limitations: serde_json::from_str::<Vec<String>>(&self.limitations_json)?,
            report_hash: self.report_hash,
            rejection_reason: self.rejection_reason,
            created_at: parse_date(&self.created_at)?,
            claimed_at: parse_optional_date(self.claimed_at)?,
            submitted_at: parse_optional_date(self.submitted_at)?,
            confirmed_at: parse_optional_date(self.confirmed_at)?,
        })
    }
}

fn read_investigation_row(row: &Row<'_>) -> rusqlite::Result<StoredInvestigationRow> {
    Ok(StoredInvestigationRow {
        id: row.get(0)?,
        project_id: row.get(1)?,
        repository_id: row.get(2)?,
        repository_key: row.get(3)?,
        base_ref: row.get(4)?,
        question: row.get(5)?,
        status: row.get(6)?,
        agent_profile: row.get(7)?,
        lease_expires_at: row.get(8)?,
        commit_sha: row.get(9)?,
        summary: row.get(10)?,
        findings_json: row.get(11)?,
        limitations_json: row.get(12)?,
        report_hash: row.get(13)?,
        rejection_reason: row.get(14)?,
        created_at: row.get(15)?,
        claimed_at: row.get(16)?,
        submitted_at: row.get(17)?,
        confirmed_at: row.get(18)?,
    })
}

fn parse_id<T>(value: &str, kind: &'static str) -> Result<T, StorageError>
where
    T: FromStr<Err = uuid::Error>,
{
    value.parse().map_err(|_| StorageError::InvalidId {
        kind,
        value: value.to_owned(),
    })
}

fn parse_optional_date(value: Option<String>) -> Result<Option<DateTime<Utc>>, StorageError> {
    value.map(|value| parse_date(&value)).transpose()
}

fn parse_status(value: &str) -> Result<CodeInvestigationStatus, StorageError> {
    match value {
        "requested" => Ok(CodeInvestigationStatus::Requested),
        "claimed" => Ok(CodeInvestigationStatus::Claimed),
        "submitted" => Ok(CodeInvestigationStatus::Submitted),
        "confirmed" => Ok(CodeInvestigationStatus::Confirmed),
        "rejected" => Ok(CodeInvestigationStatus::Rejected),
        _ => Err(StorageError::InvalidEnumValue {
            field: "code investigation status",
            value: value.to_owned(),
        }),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    format!("{:x}", digest.finalize())
}
