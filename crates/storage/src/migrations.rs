use rusqlite::{Connection, Transaction, TransactionBehavior};

use crate::StorageError;

const SCHEMA_VERSION: i64 = 2;

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
        0 => create_schema_v2(&transaction)?,
        1 => migrate_v1_to_v2(&transaction)?,
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
