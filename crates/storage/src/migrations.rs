use rusqlite::{Connection, Transaction, TransactionBehavior};

use crate::StorageError;

const SCHEMA_VERSION: i64 = 3;

pub(crate) fn migrate(connection: &mut Connection) -> Result<(), StorageError> {
    let current = read_schema_version(connection)?;
    if current == SCHEMA_VERSION {
        return Ok(());
    }
    if !(0..=SCHEMA_VERSION).contains(&current) {
        return Err(unsupported_schema(current));
    }

    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = read_schema_version(&transaction)?;

    if current == SCHEMA_VERSION {
        transaction.commit()?;
        return Ok(());
    }
    if !(0..=SCHEMA_VERSION).contains(&current) {
        return Err(unsupported_schema(current));
    }

    match current {
        0 => {
            create_schema_v2(&transaction)?;
            create_schema_v3(&transaction)?;
        }
        1 => {
            migrate_v1_to_v2(&transaction)?;
            create_schema_v3(&transaction)?;
        }
        2 => create_schema_v3(&transaction)?,
        _ => return Err(unsupported_schema(current)),
    }

    transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    transaction.commit()?;
    Ok(())
}

fn read_schema_version(connection: &Connection) -> Result<i64, StorageError> {
    Ok(connection.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

fn unsupported_schema(found: i64) -> StorageError {
    StorageError::UnsupportedSchema {
        found,
        supported: SCHEMA_VERSION,
    }
}

fn create_schema_v2(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    transaction.execute_batch(
        "
            CREATE TABLE projects (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE events (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id TEXT NOT NULL UNIQUE,
                project_id TEXT NOT NULL,
                occurred_at TEXT NOT NULL,
                event_type TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
            );

            CREATE INDEX events_by_project ON events(project_id, seq);
            ",
    )?;
    Ok(())
}

fn migrate_v1_to_v2(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    transaction.execute_batch("ALTER TABLE projects DROP COLUMN root_path;")?;
    Ok(())
}

fn create_schema_v3(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    transaction.execute_batch(
        "
            CREATE TABLE artifacts (
                id TEXT PRIMARY KEY NOT NULL,
                project_id TEXT NOT NULL,
                file_name TEXT NOT NULL,
                format TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                text_hash TEXT NOT NULL,
                byte_size INTEGER NOT NULL,
                import_source TEXT NOT NULL,
                anchors_json TEXT NOT NULL,
                imported_at TEXT NOT NULL,
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
            );

            CREATE INDEX artifacts_by_project ON artifacts(project_id, imported_at);

            CREATE TABLE blueprint_revisions (
                id TEXT PRIMARY KEY NOT NULL,
                project_id TEXT NOT NULL,
                revision_number INTEGER NOT NULL,
                status TEXT NOT NULL,
                content_json TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                source_artifact_ids_json TEXT NOT NULL,
                generation_source TEXT NOT NULL,
                model_provider TEXT,
                model_name TEXT,
                prompt_version TEXT,
                created_at TEXT NOT NULL,
                published_at TEXT,
                UNIQUE(project_id, revision_number),
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
            );

            CREATE INDEX blueprint_revisions_by_project
                ON blueprint_revisions(project_id, revision_number DESC);

            CREATE TABLE info_requests (
                id TEXT PRIMARY KEY NOT NULL,
                project_id TEXT NOT NULL,
                blueprint_revision_id TEXT NOT NULL,
                fingerprint TEXT NOT NULL,
                category TEXT NOT NULL,
                question TEXT NOT NULL,
                reason TEXT NOT NULL,
                example TEXT,
                default_value TEXT,
                impact_scope TEXT NOT NULL,
                due_at TEXT,
                blocking INTEGER NOT NULL,
                status TEXT NOT NULL,
                answer TEXT,
                resolved_at TEXT,
                created_at TEXT NOT NULL,
                UNIQUE(project_id, fingerprint),
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
                FOREIGN KEY (blueprint_revision_id)
                    REFERENCES blueprint_revisions(id) ON DELETE CASCADE
            );

            CREATE INDEX info_requests_by_project_status
                ON info_requests(project_id, status, created_at);

            CREATE TABLE project_facts (
                id TEXT PRIMARY KEY NOT NULL,
                project_id TEXT NOT NULL,
                fact_key TEXT NOT NULL,
                category TEXT NOT NULL,
                question TEXT NOT NULL,
                value TEXT NOT NULL,
                version INTEGER NOT NULL,
                source_info_request_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                UNIQUE(project_id, fact_key, version),
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
            );

            CREATE INDEX project_facts_by_key
                ON project_facts(project_id, fact_key, version DESC);
            ",
    )?;
    Ok(())
}
