use rusqlite::{Connection, Transaction, TransactionBehavior};

use crate::StorageError;

const SCHEMA_VERSION: i64 = 9;

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
            create_schema_v4(&transaction)?;
            create_schema_v5(&transaction)?;
            create_schema_v6(&transaction)?;
            create_schema_v7(&transaction)?;
            create_schema_v8(&transaction)?;
            create_schema_v9(&transaction)?;
        }
        1 => {
            migrate_v1_to_v2(&transaction)?;
            create_schema_v3(&transaction)?;
            create_schema_v4(&transaction)?;
            create_schema_v5(&transaction)?;
            create_schema_v6(&transaction)?;
            create_schema_v7(&transaction)?;
            create_schema_v8(&transaction)?;
            create_schema_v9(&transaction)?;
        }
        2 => {
            create_schema_v3(&transaction)?;
            create_schema_v4(&transaction)?;
            create_schema_v5(&transaction)?;
            create_schema_v6(&transaction)?;
            create_schema_v7(&transaction)?;
            create_schema_v8(&transaction)?;
            create_schema_v9(&transaction)?;
        }
        3 => {
            create_schema_v4(&transaction)?;
            create_schema_v5(&transaction)?;
            create_schema_v6(&transaction)?;
            create_schema_v7(&transaction)?;
            create_schema_v8(&transaction)?;
            create_schema_v9(&transaction)?;
        }
        4 => {
            create_schema_v5(&transaction)?;
            create_schema_v6(&transaction)?;
            create_schema_v7(&transaction)?;
            create_schema_v8(&transaction)?;
            create_schema_v9(&transaction)?;
        }
        5 => {
            create_schema_v6(&transaction)?;
            create_schema_v7(&transaction)?;
            create_schema_v8(&transaction)?;
            create_schema_v9(&transaction)?;
        }
        6 => {
            create_schema_v7(&transaction)?;
            create_schema_v8(&transaction)?;
            create_schema_v9(&transaction)?;
        }
        7 => {
            create_schema_v8(&transaction)?;
            create_schema_v9(&transaction)?;
        }
        8 => create_schema_v9(&transaction)?,
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

fn create_schema_v4(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    transaction.execute_batch(
        "
            CREATE TABLE plan_revisions (
                id TEXT PRIMARY KEY NOT NULL,
                project_id TEXT NOT NULL,
                revision_number INTEGER NOT NULL,
                blueprint_revision_id TEXT NOT NULL,
                blueprint_content_hash TEXT NOT NULL,
                template_version TEXT NOT NULL,
                status TEXT NOT NULL,
                content_json TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                created_at TEXT NOT NULL,
                published_at TEXT,
                UNIQUE(project_id, revision_number),
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
                FOREIGN KEY (blueprint_revision_id)
                    REFERENCES blueprint_revisions(id) ON DELETE RESTRICT
            );

            CREATE INDEX plan_revisions_by_project
                ON plan_revisions(project_id, revision_number DESC);

            CREATE TABLE task_specs (
                id TEXT PRIMARY KEY NOT NULL,
                plan_revision_id TEXT NOT NULL,
                task_key TEXT NOT NULL,
                contract_hash TEXT NOT NULL,
                kind TEXT NOT NULL,
                spec_json TEXT NOT NULL,
                UNIQUE(plan_revision_id, task_key),
                FOREIGN KEY (plan_revision_id) REFERENCES plan_revisions(id) ON DELETE CASCADE
            );

            CREATE INDEX task_specs_by_plan ON task_specs(plan_revision_id, task_key);

            CREATE TABLE work_items (
                id TEXT PRIMARY KEY NOT NULL,
                project_id TEXT NOT NULL,
                plan_revision_id TEXT NOT NULL,
                task_spec_id TEXT NOT NULL,
                task_key TEXT NOT NULL,
                status TEXT NOT NULL,
                contract_state TEXT NOT NULL,
                priority INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                completed_at TEXT,
                UNIQUE(project_id, plan_revision_id, task_key),
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
                FOREIGN KEY (plan_revision_id) REFERENCES plan_revisions(id) ON DELETE CASCADE,
                FOREIGN KEY (task_spec_id) REFERENCES task_specs(id) ON DELETE CASCADE
            );

            CREATE INDEX work_items_by_project_status
                ON work_items(project_id, status, priority DESC, task_key ASC);

            CREATE TABLE leases (
                id TEXT PRIMARY KEY NOT NULL,
                work_item_id TEXT NOT NULL,
                agent_profile TEXT NOT NULL,
                token_hash TEXT NOT NULL,
                status TEXT NOT NULL,
                issued_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                last_heartbeat_at TEXT NOT NULL,
                released_at TEXT,
                FOREIGN KEY (work_item_id) REFERENCES work_items(id) ON DELETE CASCADE
            );

            CREATE UNIQUE INDEX one_active_lease_per_work_item
                ON leases(work_item_id) WHERE status = 'active';

            CREATE TABLE work_runs (
                id TEXT PRIMARY KEY NOT NULL,
                work_item_id TEXT NOT NULL,
                lease_id TEXT NOT NULL,
                agent_profile TEXT NOT NULL,
                claim_request_id TEXT NOT NULL,
                status TEXT NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                UNIQUE(claim_request_id),
                FOREIGN KEY (work_item_id) REFERENCES work_items(id) ON DELETE CASCADE,
                FOREIGN KEY (lease_id) REFERENCES leases(id) ON DELETE CASCADE
            );

            CREATE INDEX work_runs_by_work_item ON work_runs(work_item_id, started_at DESC);

            CREATE TABLE work_reports (
                id TEXT PRIMARY KEY NOT NULL,
                work_item_id TEXT NOT NULL,
                work_run_id TEXT NOT NULL,
                contract_hash TEXT NOT NULL,
                idempotency_key_hash TEXT NOT NULL,
                report_json TEXT NOT NULL,
                report_hash TEXT NOT NULL,
                accepted INTEGER NOT NULL,
                rejection_reasons_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                UNIQUE(work_item_id, idempotency_key_hash),
                FOREIGN KEY (work_item_id) REFERENCES work_items(id) ON DELETE CASCADE,
                FOREIGN KEY (work_run_id) REFERENCES work_runs(id) ON DELETE CASCADE
            );

            CREATE INDEX work_reports_by_work_item
                ON work_reports(work_item_id, created_at DESC);

            CREATE TABLE work_blockers (
                id TEXT PRIMARY KEY NOT NULL,
                work_item_id TEXT NOT NULL,
                reason TEXT NOT NULL,
                needed_input_json TEXT NOT NULL,
                proposal TEXT,
                created_at TEXT NOT NULL,
                resolved_at TEXT,
                FOREIGN KEY (work_item_id) REFERENCES work_items(id) ON DELETE CASCADE
            );

            CREATE INDEX work_blockers_by_work_item
                ON work_blockers(work_item_id, created_at DESC);
            ",
    )?;
    Ok(())
}

fn create_schema_v5(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    transaction.execute_batch(
        "
            CREATE TABLE credentials (
                id TEXT PRIMARY KEY NOT NULL,
                project_id TEXT NOT NULL,
                name TEXT NOT NULL,
                provider TEXT NOT NULL,
                kind TEXT NOT NULL,
                environment TEXT NOT NULL,
                secret_ref TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
            );

            CREATE INDEX credentials_by_project ON credentials(project_id, provider, environment);

            CREATE TABLE repositories (
                id TEXT PRIMARY KEY NOT NULL,
                project_id TEXT NOT NULL,
                repository_key TEXT NOT NULL,
                name TEXT NOT NULL,
                local_path TEXT NOT NULL,
                remote_url TEXT NOT NULL,
                default_branch TEXT NOT NULL,
                protected_branches_json TEXT NOT NULL,
                auth_mode TEXT NOT NULL,
                credential_id TEXT,
                sync_policy TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(project_id, repository_key),
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
                FOREIGN KEY (credential_id) REFERENCES credentials(id) ON DELETE SET NULL
            );

            CREATE INDEX repositories_by_project ON repositories(project_id, repository_key);

            CREATE TABLE capability_states (
                project_id TEXT NOT NULL,
                capability_id TEXT NOT NULL,
                name TEXT NOT NULL,
                provider TEXT NOT NULL,
                environment TEXT NOT NULL,
                status TEXT NOT NULL,
                credential_id TEXT,
                note TEXT,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (project_id, capability_id, environment),
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
                FOREIGN KEY (credential_id) REFERENCES credentials(id) ON DELETE SET NULL
            );

            CREATE INDEX capability_states_by_project
                ON capability_states(project_id, environment, capability_id);

            CREATE TABLE git_syncs (
                id TEXT PRIMARY KEY NOT NULL,
                project_id TEXT NOT NULL,
                work_item_id TEXT NOT NULL,
                repository_id TEXT NOT NULL,
                branch TEXT NOT NULL,
                commit_sha TEXT NOT NULL,
                status TEXT NOT NULL,
                error_message TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
                FOREIGN KEY (work_item_id) REFERENCES work_items(id) ON DELETE CASCADE,
                FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE
            );

            CREATE INDEX git_syncs_by_work_item ON git_syncs(work_item_id, created_at DESC);
            CREATE INDEX git_syncs_by_project ON git_syncs(project_id, created_at DESC);

            CREATE TABLE project_settings (
                project_id TEXT NOT NULL,
                setting_key TEXT NOT NULL,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (project_id, setting_key),
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
            );
            ",
    )?;
    Ok(())
}

fn create_schema_v6(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    transaction.execute_batch(
        "
            CREATE TABLE repository_verification_commands (
                repository_id TEXT NOT NULL,
                name TEXT NOT NULL,
                executable TEXT NOT NULL,
                args_json TEXT NOT NULL,
                timeout_seconds INTEGER NOT NULL,
                position INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (repository_id, name),
                UNIQUE (repository_id, position),
                FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE
            );

            CREATE INDEX repository_verification_commands_by_position
                ON repository_verification_commands(repository_id, position);

            ALTER TABLE work_runs ADD COLUMN verification_snapshot_json TEXT;
            ALTER TABLE work_runs ADD COLUMN verification_snapshot_hash TEXT;

            ALTER TABLE work_reports
                ADD COLUMN verification_status TEXT NOT NULL DEFAULT 'not_required';
            ALTER TABLE work_reports ADD COLUMN verification_json TEXT;
            ALTER TABLE work_reports ADD COLUMN verification_started_at TEXT;
            ALTER TABLE work_reports ADD COLUMN verification_finished_at TEXT;

            CREATE UNIQUE INDEX work_reports_one_running_verification
                ON work_reports(work_item_id)
                WHERE verification_status = 'running';
            ",
    )?;
    Ok(())
}

fn create_schema_v7(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    transaction.execute_batch(
        "
            CREATE TABLE review_configurations (
                project_id TEXT PRIMARY KEY NOT NULL,
                enabled INTEGER NOT NULL,
                base_url TEXT NOT NULL,
                model TEXT NOT NULL,
                credential_id TEXT,
                blocking_severities_json TEXT NOT NULL,
                consent_fingerprint TEXT,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
                FOREIGN KEY (credential_id) REFERENCES credentials(id) ON DELETE SET NULL
            );

            CREATE TABLE review_standards (
                id TEXT PRIMARY KEY NOT NULL,
                project_id TEXT NOT NULL,
                name TEXT NOT NULL,
                scope_kind TEXT NOT NULL,
                repository_key TEXT,
                enabled INTEGER NOT NULL,
                position INTEGER NOT NULL,
                current_revision_id TEXT NOT NULL,
                deleted_at TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
            );

            CREATE INDEX review_standards_by_project
                ON review_standards(project_id, deleted_at, position);

            CREATE TABLE review_standard_revisions (
                id TEXT PRIMARY KEY NOT NULL,
                standard_id TEXT NOT NULL,
                project_id TEXT NOT NULL,
                revision_number INTEGER NOT NULL,
                source TEXT NOT NULL,
                file_name TEXT NOT NULL,
                format TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                text_hash TEXT NOT NULL,
                character_count INTEGER NOT NULL,
                storage_relative_path TEXT NOT NULL,
                created_at TEXT NOT NULL,
                UNIQUE(standard_id, revision_number),
                FOREIGN KEY (standard_id) REFERENCES review_standards(id) ON DELETE CASCADE,
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
            );

            CREATE INDEX review_standard_revisions_by_standard
                ON review_standard_revisions(standard_id, revision_number DESC);

            ALTER TABLE work_runs ADD COLUMN review_snapshot_json TEXT;
            ALTER TABLE work_runs ADD COLUMN review_snapshot_hash TEXT;

            ALTER TABLE work_reports
                ADD COLUMN review_status TEXT NOT NULL DEFAULT 'not_required';
            ALTER TABLE work_reports ADD COLUMN review_json TEXT;
            ALTER TABLE work_reports ADD COLUMN review_started_at TEXT;
            ALTER TABLE work_reports ADD COLUMN review_finished_at TEXT;
            ALTER TABLE work_reports ADD COLUMN review_waiver_json TEXT;

            CREATE UNIQUE INDEX work_reports_one_running_review
                ON work_reports(work_item_id)
                WHERE review_status = 'running';
            ",
    )?;
    Ok(())
}

fn create_schema_v8(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    transaction.execute_batch(
        "
            CREATE TABLE pull_requests (
                id TEXT PRIMARY KEY NOT NULL,
                project_id TEXT NOT NULL,
                work_item_id TEXT NOT NULL,
                report_id TEXT NOT NULL UNIQUE,
                repository_id TEXT NOT NULL,
                number INTEGER NOT NULL CHECK (number > 0),
                html_url TEXT NOT NULL,
                title TEXT NOT NULL,
                head_branch TEXT NOT NULL,
                base_branch TEXT NOT NULL,
                state TEXT NOT NULL,
                draft INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(repository_id, number),
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
                FOREIGN KEY (work_item_id) REFERENCES work_items(id) ON DELETE CASCADE,
                FOREIGN KEY (report_id) REFERENCES work_reports(id) ON DELETE CASCADE,
                FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE
            );

            CREATE INDEX pull_requests_by_project
                ON pull_requests(project_id, updated_at DESC);
            ",
    )?;
    Ok(())
}

fn create_schema_v9(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    transaction.execute_batch(
        "
            CREATE TABLE pull_request_insights (
                pull_request_id TEXT PRIMARY KEY NOT NULL,
                project_id TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                synced_at TEXT NOT NULL,
                FOREIGN KEY (pull_request_id) REFERENCES pull_requests(id) ON DELETE CASCADE,
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
            );

            CREATE INDEX pull_request_insights_by_project
                ON pull_request_insights(project_id, synced_at DESC);
            ",
    )?;
    Ok(())
}
