use rusqlite::{Connection, TransactionBehavior};

use crate::StorageError;

const SCHEMA_VERSION: i64 = 1;

pub(crate) fn migrate(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current: i64 = transaction.pragma_query_value(None, "user_version", |row| row.get(0))?;

    match current {
        0 => {
            transaction.execute_batch(
                "
            CREATE TABLE projects (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                root_path TEXT NOT NULL,
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
            transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        SCHEMA_VERSION => {}
        _ => {
            return Err(StorageError::UnsupportedSchema {
                found: current,
                supported: SCHEMA_VERSION,
            });
        }
    }

    transaction.commit()?;
    Ok(())
}
