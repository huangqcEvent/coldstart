//! Pull Request 与验收快照的持久化。
//!
//! 外部 GitHub 状态先由适配器读取并构造有界快照，再整体写入；失败刷新不得把
//! 半套 Checks、Review 或部署数据当成完整验收结果。

use std::str::FromStr;

use chrono::{DateTime, Utc};
use coldstart_core::{
    DomainEvent, DomainEventKind, ProjectId, PullRequestId, PullRequestInsights, PullRequestRecord,
    PullRequestState,
};
use rusqlite::{OptionalExtension, Row, Transaction, TransactionBehavior, params};
use sha2::{Digest, Sha256};

use crate::{
    Database, StorageError,
    database::{insert_event, project_exists},
};

impl Database {
    /// 保存 PR 创建或状态快照及审计事件，保持任务关联和外部编号一致。
    pub fn save_pull_request(&mut self, record: &PullRequestRecord) -> Result<(), StorageError> {
        record.validate()?;
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !project_exists(&transaction, &record.project_id)? {
            return Err(StorageError::ProjectNotFound(record.project_id));
        }
        ensure_pull_request_scope(&transaction, record)?;

        let report_owner = transaction
            .query_row(
                "SELECT id FROM pull_requests WHERE report_id = ?1",
                [record.report_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if report_owner
            .as_deref()
            .is_some_and(|id| id != record.id.to_string())
        {
            return Err(StorageError::PullRequestReportAlreadyLinked(
                record.report_id,
            ));
        }

        let number_owner = transaction
            .query_row(
                "SELECT id FROM pull_requests WHERE repository_id = ?1 AND number = ?2",
                params![record.repository_id.to_string(), record.number as i64],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if number_owner
            .as_deref()
            .is_some_and(|id| id != record.id.to_string())
        {
            return Err(StorageError::PullRequestNumberAlreadyLinked {
                repository_id: record.repository_id,
                number: record.number,
            });
        }

        let existing = transaction
            .query_row(
                "SELECT state, created_at FROM pull_requests WHERE id = ?1 AND project_id = ?2",
                params![record.id.to_string(), record.project_id.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let created_at = existing
            .as_ref()
            .map(|(_, created_at)| created_at.clone())
            .unwrap_or_else(|| record.created_at.to_rfc3339());

        transaction.execute(
            "INSERT INTO pull_requests (
                id, project_id, work_item_id, report_id, repository_id, number,
                html_url, title, head_branch, base_branch, state, draft,
                created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(id) DO UPDATE SET
                html_url = excluded.html_url,
                title = excluded.title,
                head_branch = excluded.head_branch,
                base_branch = excluded.base_branch,
                state = excluded.state,
                draft = excluded.draft,
                updated_at = excluded.updated_at",
            params![
                record.id.to_string(),
                record.project_id.to_string(),
                record.work_item_id.to_string(),
                record.report_id.to_string(),
                record.repository_id.to_string(),
                record.number as i64,
                record.html_url.trim(),
                record.title.trim(),
                record.head_branch,
                record.base_branch,
                record.state.as_str(),
                record.draft,
                created_at,
                record.updated_at.to_rfc3339(),
            ],
        )?;

        match existing {
            None => insert_event(
                &transaction,
                &DomainEvent::new(
                    record.project_id,
                    DomainEventKind::PullRequestOpened {
                        pull_request_id: record.id,
                        work_item_id: record.work_item_id,
                        repository_id: record.repository_id,
                        number: record.number,
                        state: record.state.as_str().to_owned(),
                    },
                ),
            )?,
            Some((previous_state, _)) if previous_state != record.state.as_str() => insert_event(
                &transaction,
                &DomainEvent::new(
                    record.project_id,
                    DomainEventKind::PullRequestStateChanged {
                        pull_request_id: record.id,
                        work_item_id: record.work_item_id,
                        number: record.number,
                        previous_state,
                        state: record.state.as_str().to_owned(),
                    },
                ),
            )?,
            Some(_) => {}
        }

        transaction.commit()?;
        Ok(())
    }

    pub fn list_pull_requests(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<PullRequestRecord>, StorageError> {
        let mut statement = self.connection().prepare(
            "SELECT id, project_id, work_item_id, report_id, repository_id, number,
                    html_url, title, head_branch, base_branch, state, draft,
                    created_at, updated_at
             FROM pull_requests WHERE project_id = ?1
             ORDER BY created_at DESC, id DESC",
        )?;
        let rows = statement.query_map([project_id.to_string()], read_pull_request_row)?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?.into_record()?);
        }
        Ok(records)
    }

    pub fn load_pull_request(
        &self,
        project_id: &ProjectId,
        pull_request_id: &PullRequestId,
    ) -> Result<Option<PullRequestRecord>, StorageError> {
        let row = self
            .connection()
            .query_row(
                "SELECT id, project_id, work_item_id, report_id, repository_id, number,
                        html_url, title, head_branch, base_branch, state, draft,
                        created_at, updated_at
                 FROM pull_requests WHERE project_id = ?1 AND id = ?2",
                params![project_id.to_string(), pull_request_id.to_string()],
                read_pull_request_row,
            )
            .optional()?;
        row.map(StoredPullRequestRow::into_record).transpose()
    }

    pub fn save_pull_request_insights(
        &mut self,
        insights: &PullRequestInsights,
    ) -> Result<bool, StorageError> {
        insights.validate()?;
        let content_hash = insights_content_hash(insights)?;
        let payload_json = serde_json::to_string(insights)?;
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let pull_request = transaction
            .query_row(
                "SELECT project_id, work_item_id, number FROM pull_requests WHERE id = ?1",
                [insights.pull_request_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((project_id, work_item_id, number)) = pull_request else {
            return Err(StorageError::PullRequestNotFound(insights.pull_request_id));
        };
        let existing_hash = transaction
            .query_row(
                "SELECT content_hash FROM pull_request_insights WHERE pull_request_id = ?1",
                [insights.pull_request_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let changed = existing_hash.as_deref() != Some(content_hash.as_str());
        transaction.execute(
            "INSERT INTO pull_request_insights (
                pull_request_id, project_id, payload_json, content_hash, synced_at
             ) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(pull_request_id) DO UPDATE SET
                payload_json = excluded.payload_json,
                content_hash = excluded.content_hash,
                synced_at = excluded.synced_at",
            params![
                insights.pull_request_id.to_string(),
                project_id,
                payload_json,
                content_hash,
                insights.synced_at.to_rfc3339(),
            ],
        )?;
        if changed {
            let project_id = parse_id(&project_id, "project")?;
            let work_item_id = parse_id(&work_item_id, "work item")?;
            let number = u64::try_from(number).map_err(|_| StorageError::InvalidId {
                kind: "pull request number",
                value: number.to_string(),
            })?;
            let summary = insights.check_summary();
            insert_event(
                &transaction,
                &DomainEvent::new(
                    project_id,
                    DomainEventKind::PullRequestInsightsSynced {
                        pull_request_id: insights.pull_request_id,
                        work_item_id,
                        number,
                        checks_state: insights.checks_state().as_str().to_owned(),
                        checks_total: summary.total,
                        checks_failed: summary.failed,
                        review_count: insights.reviews.len(),
                        comment_count: insights.review_comments.len(),
                    },
                ),
            )?;
        }
        transaction.commit()?;
        Ok(changed)
    }

    pub fn load_pull_request_insights(
        &self,
        project_id: &ProjectId,
        pull_request_id: &PullRequestId,
    ) -> Result<Option<PullRequestInsights>, StorageError> {
        let payload = self
            .connection()
            .query_row(
                "SELECT payload_json FROM pull_request_insights
                 WHERE project_id = ?1 AND pull_request_id = ?2",
                params![project_id.to_string(), pull_request_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(payload) = payload else {
            return Ok(None);
        };
        let insights: PullRequestInsights = serde_json::from_str(&payload)?;
        insights.validate()?;
        Ok(Some(insights))
    }
}

fn insights_content_hash(insights: &PullRequestInsights) -> Result<String, StorageError> {
    let content = serde_json::json!({
        "checks": insights.checks,
        "reviews": insights.reviews,
        "reviewComments": insights.review_comments,
    });
    let bytes = serde_json::to_vec(&content)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn ensure_pull_request_scope(
    transaction: &Transaction<'_>,
    record: &PullRequestRecord,
) -> Result<(), StorageError> {
    let valid: bool = transaction.query_row(
        "SELECT EXISTS(
            SELECT 1
            FROM work_items w
            JOIN work_reports r ON r.work_item_id = w.id
            JOIN repositories repo ON repo.id = ?4 AND repo.project_id = w.project_id
            WHERE w.id = ?1 AND w.project_id = ?2 AND r.id = ?3
         )",
        params![
            record.work_item_id.to_string(),
            record.project_id.to_string(),
            record.report_id.to_string(),
            record.repository_id.to_string(),
        ],
        |row| row.get(0),
    )?;
    if valid {
        Ok(())
    } else {
        Err(StorageError::PullRequestScopeMismatch(record.project_id))
    }
}

fn read_pull_request_row(row: &Row<'_>) -> rusqlite::Result<StoredPullRequestRow> {
    Ok(StoredPullRequestRow {
        id: row.get(0)?,
        project_id: row.get(1)?,
        work_item_id: row.get(2)?,
        report_id: row.get(3)?,
        repository_id: row.get(4)?,
        number: row.get(5)?,
        html_url: row.get(6)?,
        title: row.get(7)?,
        head_branch: row.get(8)?,
        base_branch: row.get(9)?,
        state: row.get(10)?,
        draft: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

struct StoredPullRequestRow {
    id: String,
    project_id: String,
    work_item_id: String,
    report_id: String,
    repository_id: String,
    number: i64,
    html_url: String,
    title: String,
    head_branch: String,
    base_branch: String,
    state: String,
    draft: bool,
    created_at: String,
    updated_at: String,
}

impl StoredPullRequestRow {
    fn into_record(self) -> Result<PullRequestRecord, StorageError> {
        Ok(PullRequestRecord {
            id: parse_id(&self.id, "pull request")?,
            project_id: parse_id(&self.project_id, "project")?,
            work_item_id: parse_id(&self.work_item_id, "work item")?,
            report_id: parse_id(&self.report_id, "work report")?,
            repository_id: parse_id(&self.repository_id, "repository")?,
            number: u64::try_from(self.number).map_err(|_| StorageError::InvalidId {
                kind: "pull request number",
                value: self.number.to_string(),
            })?,
            html_url: self.html_url,
            title: self.title,
            head_branch: self.head_branch,
            base_branch: self.base_branch,
            state: PullRequestState::parse(&self.state).ok_or_else(|| {
                StorageError::InvalidEnumValue {
                    field: "pull request state",
                    value: self.state,
                }
            })?,
            draft: self.draft,
            created_at: parse_date(&self.created_at)?,
            updated_at: parse_date(&self.updated_at)?,
        })
    }
}

fn parse_id<T>(value: &str, kind: &'static str) -> Result<T, StorageError>
where
    T: FromStr,
{
    value.parse().map_err(|_| StorageError::InvalidId {
        kind,
        value: value.to_owned(),
    })
}

fn parse_date(value: &str) -> Result<DateTime<Utc>, StorageError> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}
