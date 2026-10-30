use std::{
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use coldstart_core::{DomainEvent, Project, ProjectId};
use rusqlite::{
    Connection, ErrorCode, OptionalExtension, Transaction, TransactionBehavior, params,
};
use thiserror::Error;
use uuid::Uuid;

use crate::migrations;

const BUSY_TIMEOUT: Duration = Duration::from_millis(5_000);
const WAL_RETRY_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid project id in storage: {0}")]
    InvalidProjectId(#[from] uuid::Error),
    #[error("invalid timestamp in storage: {0}")]
    InvalidTimestamp(#[from] chrono::ParseError),
    #[error("project {0} already exists")]
    ProjectAlreadyExists(ProjectId),
    #[error("project {0} does not exist")]
    ProjectNotFound(ProjectId),
    #[error("event {0} already exists")]
    EventAlreadyExists(Uuid),
    #[error("database schema version {found} is newer than supported version {supported}")]
    UnsupportedSchema { found: i64, supported: i64 },
    #[error("project root path is not valid UTF-8")]
    NonUtf8ProjectRoot,
    #[error("SQLite did not enable WAL mode; active journal mode is {0}")]
    WalUnavailable(String),
}

pub struct Database {
    connection: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }

        let mut connection = Connection::open(path)?;
        connection.busy_timeout(BUSY_TIMEOUT)?;
        connection.pragma_update(None, "foreign_keys", true)?;
        enable_wal(&connection)?;
        migrations::migrate(&mut connection)?;

        Ok(Self { connection })
    }

    pub fn initialize_project(&mut self, project: &Project) -> Result<(), StorageError> {
        let root_path = project
            .root_path
            .to_str()
            .ok_or(StorageError::NonUtf8ProjectRoot)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        if project_exists(&transaction, &project.id)? {
            return Err(StorageError::ProjectAlreadyExists(project.id));
        }

        transaction.execute(
            "INSERT INTO projects (id, name, root_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                project.id.to_string(),
                project.name,
                root_path,
                project.created_at.to_rfc3339(),
            ],
        )?;
        insert_event(&transaction, &DomainEvent::project_initialized(project))?;
        transaction.commit()?;
        Ok(())
    }

    pub fn load_project(&self, id: &ProjectId) -> Result<Option<Project>, StorageError> {
        let row = self
            .connection
            .query_row(
                "SELECT id, name, root_path, created_at FROM projects WHERE id = ?1",
                [id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;

        let Some((stored_id, name, root_path, created_at)) = row else {
            return Ok(None);
        };
        let created_at: DateTime<Utc> =
            DateTime::parse_from_rfc3339(&created_at)?.with_timezone(&Utc);

        Ok(Some(Project {
            id: stored_id.parse()?,
            name,
            root_path: root_path.into(),
            created_at,
        }))
    }

    pub fn append_event(&mut self, event: &DomainEvent) -> Result<(), StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        if !project_exists(&transaction, &event.project_id)? {
            return Err(StorageError::ProjectNotFound(event.project_id));
        }
        if event_exists(&transaction, event.event_id)? {
            return Err(StorageError::EventAlreadyExists(event.event_id));
        }

        insert_event(&transaction, event)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn event_count(&self, project_id: &ProjectId) -> Result<u64, StorageError> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM events WHERE project_id = ?1",
            [project_id.to_string()],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }
}

fn enable_wal(connection: &Connection) -> Result<(), StorageError> {
    let started = Instant::now();
    loop {
        match connection.query_row("PRAGMA journal_mode = WAL", [], |row| {
            row.get::<_, String>(0)
        }) {
            Ok(mode) if mode.eq_ignore_ascii_case("wal") => return Ok(()),
            Ok(mode) => return Err(StorageError::WalUnavailable(mode)),
            Err(error)
                if matches!(
                    error.sqlite_error_code(),
                    Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked)
                ) && started.elapsed() < BUSY_TIMEOUT =>
            {
                thread::sleep(WAL_RETRY_INTERVAL);
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn project_exists(transaction: &Transaction<'_>, id: &ProjectId) -> Result<bool, StorageError> {
    let exists = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE id = ?1)",
        [id.to_string()],
        |row| row.get(0),
    )?;
    Ok(exists)
}

fn event_exists(transaction: &Transaction<'_>, id: Uuid) -> Result<bool, StorageError> {
    let exists = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM events WHERE event_id = ?1)",
        [id.to_string()],
        |row| row.get(0),
    )?;
    Ok(exists)
}

fn insert_event(transaction: &Transaction<'_>, event: &DomainEvent) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO events (event_id, project_id, occurred_at, event_type, payload_json)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            event.event_id.to_string(),
            event.project_id.to_string(),
            event.occurred_at.to_rfc3339(),
            event.kind.type_name(),
            serde_json::to_string(&event.kind)?,
        ],
    )?;
    Ok(())
}
