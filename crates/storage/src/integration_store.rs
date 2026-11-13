//! 仓库、凭据元数据与能力状态的持久化。
//!
//! 外部服务的 Token 和 Secret 只以系统凭据库引用形式保存；仓库映射、同步策略
//! 与能力环境在事务内维护，保证工作区读取看到一致配置。

use std::{path::PathBuf, str::FromStr};

use chrono::{DateTime, Utc};
use coldstart_core::{
    CapabilityState, CapabilityStatus, CredentialEnvironment, CredentialId, CredentialKind,
    CredentialMetadata, DomainEvent, DomainEventKind, GitSyncId, GitSyncRecord, GitSyncStatus,
    ProjectId, Repository, RepositoryAuthMode, RepositoryId, RepositorySyncPolicy, WorkItemId,
};
use rusqlite::{OptionalExtension, Row, Transaction, TransactionBehavior, params};

use crate::{
    Database, StorageError,
    database::{insert_event, project_exists},
};

impl Database {
    /// 保存仓库映射并校验键、路径和凭据引用，保持已有身份。
    pub fn save_repository(&mut self, repository: &Repository) -> Result<(), StorageError> {
        repository.validate()?;
        let protected_branches_json = serde_json::to_string(&repository.protected_branches)?;
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_project(&transaction, &repository.project_id)?;
        ensure_credential_project(
            &transaction,
            &repository.project_id,
            repository.credential_id,
        )?;

        if let Some(existing_id) = transaction
            .query_row(
                "SELECT id FROM repositories WHERE project_id = ?1 AND repository_key = ?2",
                params![repository.project_id.to_string(), repository.key.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            && existing_id != repository.id.to_string()
        {
            return Err(StorageError::RepositoryKeyAlreadyExists {
                project_id: repository.project_id,
                key: repository.key.clone(),
            });
        }

        let created_at = transaction
            .query_row(
                "SELECT created_at FROM repositories WHERE id = ?1 AND project_id = ?2",
                params![repository.id.to_string(), repository.project_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .unwrap_or_else(|| repository.created_at.to_rfc3339());

        transaction.execute(
            "INSERT INTO repositories (
                id, project_id, repository_key, name, local_path, remote_url,
                default_branch, protected_branches_json, auth_mode, credential_id,
                sync_policy, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
                repository_key = excluded.repository_key,
                name = excluded.name,
                local_path = excluded.local_path,
                remote_url = excluded.remote_url,
                default_branch = excluded.default_branch,
                protected_branches_json = excluded.protected_branches_json,
                auth_mode = excluded.auth_mode,
                credential_id = excluded.credential_id,
                sync_policy = excluded.sync_policy,
                updated_at = excluded.updated_at",
            params![
                repository.id.to_string(),
                repository.project_id.to_string(),
                repository.key,
                repository.name,
                repository.local_path.to_string_lossy(),
                repository.remote_url,
                repository.default_branch,
                protected_branches_json,
                repository.auth_mode.as_str(),
                repository.credential_id.map(|id| id.to_string()),
                repository.sync_policy.as_str(),
                created_at,
                repository.updated_at.to_rfc3339(),
            ],
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                repository.project_id,
                DomainEventKind::RepositorySaved {
                    repository_id: repository.id,
                    repository_key: repository.key.clone(),
                    remote_url: repository.remote_url.clone(),
                },
            ),
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn list_repositories(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<Repository>, StorageError> {
        let mut statement = self.connection().prepare(
            "SELECT id, project_id, repository_key, name, local_path, remote_url,
                    default_branch, protected_branches_json, auth_mode, credential_id,
                    sync_policy, created_at, updated_at
             FROM repositories WHERE project_id = ?1
             ORDER BY repository_key ASC",
        )?;
        let rows = statement.query_map([project_id.to_string()], read_repository_row)?;
        let mut repositories = Vec::new();
        for row in rows {
            repositories.push(row?.into_repository()?);
        }
        Ok(repositories)
    }

    pub fn repository_by_key(
        &self,
        project_id: &ProjectId,
        key: &str,
    ) -> Result<Option<Repository>, StorageError> {
        let row = self
            .connection()
            .query_row(
                "SELECT id, project_id, repository_key, name, local_path, remote_url,
                        default_branch, protected_branches_json, auth_mode, credential_id,
                        sync_policy, created_at, updated_at
                 FROM repositories WHERE project_id = ?1 AND repository_key = ?2",
                params![project_id.to_string(), key],
                read_repository_row,
            )
            .optional()?;
        row.map(StoredRepositoryRow::into_repository).transpose()
    }

    /// 保存凭据元数据，仅存储系统凭据库引用和展示所需字段。
    pub fn save_credential(&mut self, credential: &CredentialMetadata) -> Result<(), StorageError> {
        credential.validate()?;
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_project(&transaction, &credential.project_id)?;
        if let Some(existing_id) = transaction
            .query_row(
                "SELECT id FROM credentials WHERE secret_ref = ?1",
                [credential.secret_ref.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            && existing_id != credential.id.to_string()
        {
            return Err(StorageError::CredentialSecretRefAlreadyExists(
                credential.secret_ref.clone(),
            ));
        }
        if let Some(existing_project) = transaction
            .query_row(
                "SELECT project_id FROM credentials WHERE id = ?1",
                [credential.id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            && existing_project != credential.project_id.to_string()
        {
            return Err(StorageError::CredentialProjectMismatch(credential.id));
        }

        let created_at = transaction
            .query_row(
                "SELECT created_at FROM credentials WHERE id = ?1",
                [credential.id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .unwrap_or_else(|| credential.created_at.to_rfc3339());
        transaction.execute(
            "INSERT INTO credentials (
                id, project_id, name, provider, kind, environment, secret_ref,
                created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                provider = excluded.provider,
                kind = excluded.kind,
                environment = excluded.environment,
                secret_ref = excluded.secret_ref,
                updated_at = excluded.updated_at",
            params![
                credential.id.to_string(),
                credential.project_id.to_string(),
                credential.name,
                credential.provider,
                credential.kind.as_str(),
                credential.environment.as_str(),
                credential.secret_ref,
                created_at,
                credential.updated_at.to_rfc3339(),
            ],
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                credential.project_id,
                DomainEventKind::CredentialSaved {
                    credential_id: credential.id,
                    provider: credential.provider.clone(),
                    environment: credential.environment.as_str().to_owned(),
                },
            ),
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn list_credentials(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<CredentialMetadata>, StorageError> {
        let mut statement = self.connection().prepare(
            "SELECT id, project_id, name, provider, kind, environment, secret_ref,
                    created_at, updated_at
             FROM credentials WHERE project_id = ?1
             ORDER BY created_at ASC, id ASC",
        )?;
        let rows = statement.query_map([project_id.to_string()], read_credential_row)?;
        let mut credentials = Vec::new();
        for row in rows {
            credentials.push(row?.into_credential()?);
        }
        Ok(credentials)
    }

    pub fn credential(
        &self,
        project_id: &ProjectId,
        credential_id: &CredentialId,
    ) -> Result<Option<CredentialMetadata>, StorageError> {
        let row = self
            .connection()
            .query_row(
                "SELECT id, project_id, name, provider, kind, environment, secret_ref,
                        created_at, updated_at
                 FROM credentials WHERE project_id = ?1 AND id = ?2",
                params![project_id.to_string(), credential_id.to_string()],
                read_credential_row,
            )
            .optional()?;
        row.map(StoredCredentialRow::into_credential).transpose()
    }

    pub fn delete_credential(
        &mut self,
        project_id: &ProjectId,
        credential_id: &CredentialId,
    ) -> Result<bool, StorageError> {
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let deleted = transaction.execute(
            "DELETE FROM credentials WHERE project_id = ?1 AND id = ?2",
            params![project_id.to_string(), credential_id.to_string()],
        )?;
        if deleted == 0 {
            transaction.commit()?;
            return Ok(false);
        }
        insert_event(
            &transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::CredentialDeleted {
                    credential_id: *credential_id,
                },
            ),
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn save_capability_state(&mut self, state: &CapabilityState) -> Result<(), StorageError> {
        state.validate()?;
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_project(&transaction, &state.project_id)?;
        ensure_credential_project(&transaction, &state.project_id, state.credential_id)?;
        transaction.execute(
            "INSERT INTO capability_states (
                project_id, capability_id, name, provider, environment, status,
                credential_id, note, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(project_id, capability_id, environment) DO UPDATE SET
                name = excluded.name,
                provider = excluded.provider,
                status = excluded.status,
                credential_id = excluded.credential_id,
                note = excluded.note,
                updated_at = excluded.updated_at",
            params![
                state.project_id.to_string(),
                state.capability_id.as_str(),
                state.name,
                state.provider,
                state.environment,
                state.status.as_str(),
                state.credential_id.map(|id| id.to_string()),
                state.note,
                state.updated_at.to_rfc3339(),
            ],
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                state.project_id,
                DomainEventKind::CapabilityUpdated {
                    capability_id: state.capability_id.to_string(),
                    environment: state.environment.clone(),
                    status: state.status.as_str().to_owned(),
                },
            ),
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn list_capability_states(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<CapabilityState>, StorageError> {
        let mut statement = self.connection().prepare(
            "SELECT project_id, capability_id, name, provider, environment, status,
                    credential_id, note, updated_at
             FROM capability_states WHERE project_id = ?1
             ORDER BY environment ASC, capability_id ASC",
        )?;
        let rows = statement.query_map([project_id.to_string()], read_capability_row)?;
        let mut states = Vec::new();
        for row in rows {
            states.push(row?.into_capability()?);
        }
        Ok(states)
    }

    pub fn set_project_setting(
        &mut self,
        project_id: &ProjectId,
        setting_key: &str,
        value: &str,
    ) -> Result<(), StorageError> {
        let setting_key = setting_key.trim();
        if setting_key.is_empty() {
            return Err(StorageError::EmptyProjectSettingKey);
        }
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_project(&transaction, project_id)?;
        transaction.execute(
            "INSERT INTO project_settings (project_id, setting_key, value, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(project_id, setting_key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at",
            params![
                project_id.to_string(),
                setting_key,
                value,
                Utc::now().to_rfc3339()
            ],
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::ProjectSettingUpdated {
                    setting_key: setting_key.to_owned(),
                },
            ),
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn project_setting(
        &self,
        project_id: &ProjectId,
        setting_key: &str,
    ) -> Result<Option<String>, StorageError> {
        Ok(self
            .connection()
            .query_row(
                "SELECT value FROM project_settings WHERE project_id = ?1 AND setting_key = ?2",
                params![project_id.to_string(), setting_key.trim()],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// 记录任务分支提交与推送结果，不改变任务完成状态。
    pub fn record_git_sync(&mut self, sync: &GitSyncRecord) -> Result<(), StorageError> {
        if sync.branch.trim().is_empty() || sync.commit_sha.trim().is_empty() {
            return Err(StorageError::EmptyGitSyncFields);
        }
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_project(&transaction, &sync.project_id)?;
        let repository_in_project: bool = transaction.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM repositories WHERE id = ?1 AND project_id = ?2
             )",
            params![sync.repository_id.to_string(), sync.project_id.to_string()],
            |row| row.get(0),
        )?;
        let work_item_in_project: bool = transaction.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM work_items WHERE id = ?1 AND project_id = ?2
             )",
            params![sync.work_item_id.to_string(), sync.project_id.to_string()],
            |row| row.get(0),
        )?;
        if !repository_in_project || !work_item_in_project {
            return Err(StorageError::GitSyncScopeMismatch(sync.project_id));
        }
        transaction.execute(
            "INSERT INTO git_syncs (
                id, project_id, work_item_id, repository_id, branch, commit_sha,
                status, error_message, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                sync.id.to_string(),
                sync.project_id.to_string(),
                sync.work_item_id.to_string(),
                sync.repository_id.to_string(),
                sync.branch,
                sync.commit_sha,
                sync.status.as_str(),
                sync.error_message,
                sync.created_at.to_rfc3339(),
            ],
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                sync.project_id,
                DomainEventKind::GitSyncRecorded {
                    sync_id: sync.id,
                    work_item_id: sync.work_item_id,
                    repository_id: sync.repository_id,
                    status: sync.status.as_str().to_owned(),
                },
            ),
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn list_git_syncs(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<GitSyncRecord>, StorageError> {
        let mut statement = self.connection().prepare(
            "SELECT id, project_id, work_item_id, repository_id, branch, commit_sha,
                    status, error_message, created_at
             FROM git_syncs WHERE project_id = ?1
             ORDER BY created_at DESC, id DESC",
        )?;
        let rows = statement.query_map([project_id.to_string()], read_git_sync_row)?;
        let mut syncs = Vec::new();
        for row in rows {
            syncs.push(row?.into_git_sync()?);
        }
        Ok(syncs)
    }
}

fn ensure_project(
    transaction: &Transaction<'_>,
    project_id: &ProjectId,
) -> Result<(), StorageError> {
    if project_exists(transaction, project_id)? {
        Ok(())
    } else {
        Err(StorageError::ProjectNotFound(*project_id))
    }
}

fn ensure_credential_project(
    transaction: &Transaction<'_>,
    project_id: &ProjectId,
    credential_id: Option<CredentialId>,
) -> Result<(), StorageError> {
    let Some(credential_id) = credential_id else {
        return Ok(());
    };
    let stored_project = transaction
        .query_row(
            "SELECT project_id FROM credentials WHERE id = ?1",
            [credential_id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if stored_project.as_deref() != Some(project_id.to_string().as_str()) {
        return Err(StorageError::CredentialProjectMismatch(credential_id));
    }
    Ok(())
}

#[derive(Debug)]
struct StoredRepositoryRow {
    id: String,
    project_id: String,
    key: String,
    name: String,
    local_path: String,
    remote_url: String,
    default_branch: String,
    protected_branches_json: String,
    auth_mode: String,
    credential_id: Option<String>,
    sync_policy: String,
    created_at: String,
    updated_at: String,
}

impl StoredRepositoryRow {
    fn into_repository(self) -> Result<Repository, StorageError> {
        Ok(Repository {
            id: parse_id(&self.id, "repository")?,
            project_id: parse_id(&self.project_id, "project")?,
            key: self.key,
            name: self.name,
            local_path: PathBuf::from(self.local_path),
            remote_url: self.remote_url,
            default_branch: self.default_branch,
            protected_branches: serde_json::from_str(&self.protected_branches_json)?,
            auth_mode: parse_optional(
                RepositoryAuthMode::parse(&self.auth_mode),
                "repository auth mode",
                &self.auth_mode,
            )?,
            credential_id: self
                .credential_id
                .map(|id| parse_id(&id, "credential"))
                .transpose()?,
            sync_policy: parse_optional(
                RepositorySyncPolicy::parse(&self.sync_policy),
                "repository sync policy",
                &self.sync_policy,
            )?,
            created_at: parse_date(&self.created_at)?,
            updated_at: parse_date(&self.updated_at)?,
        })
    }
}

fn read_repository_row(row: &Row<'_>) -> rusqlite::Result<StoredRepositoryRow> {
    Ok(StoredRepositoryRow {
        id: row.get(0)?,
        project_id: row.get(1)?,
        key: row.get(2)?,
        name: row.get(3)?,
        local_path: row.get(4)?,
        remote_url: row.get(5)?,
        default_branch: row.get(6)?,
        protected_branches_json: row.get(7)?,
        auth_mode: row.get(8)?,
        credential_id: row.get(9)?,
        sync_policy: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

#[derive(Debug)]
struct StoredCredentialRow {
    id: String,
    project_id: String,
    name: String,
    provider: String,
    kind: String,
    environment: String,
    secret_ref: String,
    created_at: String,
    updated_at: String,
}

impl StoredCredentialRow {
    fn into_credential(self) -> Result<CredentialMetadata, StorageError> {
        Ok(CredentialMetadata {
            id: parse_id(&self.id, "credential")?,
            project_id: parse_id(&self.project_id, "project")?,
            name: self.name,
            provider: self.provider,
            kind: parse_optional(
                CredentialKind::parse(&self.kind),
                "credential kind",
                &self.kind,
            )?,
            environment: parse_optional(
                CredentialEnvironment::parse(&self.environment),
                "credential environment",
                &self.environment,
            )?,
            secret_ref: self.secret_ref,
            created_at: parse_date(&self.created_at)?,
            updated_at: parse_date(&self.updated_at)?,
        })
    }
}

fn read_credential_row(row: &Row<'_>) -> rusqlite::Result<StoredCredentialRow> {
    Ok(StoredCredentialRow {
        id: row.get(0)?,
        project_id: row.get(1)?,
        name: row.get(2)?,
        provider: row.get(3)?,
        kind: row.get(4)?,
        environment: row.get(5)?,
        secret_ref: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

#[derive(Debug)]
struct StoredCapabilityRow {
    project_id: String,
    capability_id: String,
    name: String,
    provider: String,
    environment: String,
    status: String,
    credential_id: Option<String>,
    note: Option<String>,
    updated_at: String,
}

impl StoredCapabilityRow {
    fn into_capability(self) -> Result<CapabilityState, StorageError> {
        Ok(CapabilityState {
            project_id: parse_id(&self.project_id, "project")?,
            capability_id: parse_id(&self.capability_id, "capability")?,
            name: self.name,
            provider: self.provider,
            environment: self.environment,
            status: parse_optional(
                CapabilityStatus::parse(&self.status),
                "capability status",
                &self.status,
            )?,
            credential_id: self
                .credential_id
                .map(|id| parse_id(&id, "credential"))
                .transpose()?,
            note: self.note,
            updated_at: parse_date(&self.updated_at)?,
        })
    }
}

fn read_capability_row(row: &Row<'_>) -> rusqlite::Result<StoredCapabilityRow> {
    Ok(StoredCapabilityRow {
        project_id: row.get(0)?,
        capability_id: row.get(1)?,
        name: row.get(2)?,
        provider: row.get(3)?,
        environment: row.get(4)?,
        status: row.get(5)?,
        credential_id: row.get(6)?,
        note: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

#[derive(Debug)]
struct StoredGitSyncRow {
    id: String,
    project_id: String,
    work_item_id: String,
    repository_id: String,
    branch: String,
    commit_sha: String,
    status: String,
    error_message: Option<String>,
    created_at: String,
}

impl StoredGitSyncRow {
    fn into_git_sync(self) -> Result<GitSyncRecord, StorageError> {
        Ok(GitSyncRecord {
            id: parse_id::<GitSyncId>(&self.id, "git sync")?,
            project_id: parse_id(&self.project_id, "project")?,
            work_item_id: parse_id::<WorkItemId>(&self.work_item_id, "work item")?,
            repository_id: parse_id::<RepositoryId>(&self.repository_id, "repository")?,
            branch: self.branch,
            commit_sha: self.commit_sha,
            status: parse_optional(
                GitSyncStatus::parse(&self.status),
                "git sync status",
                &self.status,
            )?,
            error_message: self.error_message,
            created_at: parse_date(&self.created_at)?,
        })
    }
}

fn read_git_sync_row(row: &Row<'_>) -> rusqlite::Result<StoredGitSyncRow> {
    Ok(StoredGitSyncRow {
        id: row.get(0)?,
        project_id: row.get(1)?,
        work_item_id: row.get(2)?,
        repository_id: row.get(3)?,
        branch: row.get(4)?,
        commit_sha: row.get(5)?,
        status: row.get(6)?,
        error_message: row.get(7)?,
        created_at: row.get(8)?,
    })
}

fn parse_optional<T>(
    value: Option<T>,
    field: &'static str,
    stored: &str,
) -> Result<T, StorageError> {
    value.ok_or_else(|| StorageError::InvalidEnumValue {
        field,
        value: stored.to_owned(),
    })
}

fn parse_id<T>(value: &str, kind: &'static str) -> Result<T, StorageError>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    value.parse().map_err(|_| StorageError::InvalidId {
        kind,
        value: value.to_owned(),
    })
}

fn parse_date(value: &str) -> Result<DateTime<Utc>, StorageError> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}
