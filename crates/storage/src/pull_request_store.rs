use std::str::FromStr;

use chrono::{DateTime, Utc};
use coldstart_core::{
    DomainEvent, DomainEventKind, ProjectId, PullRequestId, PullRequestRecord, PullRequestState,
};
use rusqlite::{OptionalExtension, Row, Transaction, TransactionBehavior, params};

use crate::{
    Database, StorageError,
    database::{insert_event, project_exists},
};

impl Database {
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
