use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    str::FromStr,
    thread,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use coldstart_core::{
    ArtifactFormat, ArtifactId, ArtifactMetadata, BlueprintContent, BlueprintGeneration,
    BlueprintRevision, BlueprintRevisionId, BlueprintStatus, BlueprintValidationError,
    DocumentAnchor, DomainEvent, DomainEventKind, GenerationSource, InfoRequest,
    InfoRequestCategory, InfoRequestDraft, InfoRequestId, InfoRequestStatus,
    IntegrationValidationError, PlanRevisionId, PlanValidationError, Project, ProjectFact,
    ProjectFactId, ProjectId, WorkItemId, validate_blueprint_structure,
};
use rusqlite::{
    Connection, ErrorCode, OptionalExtension, Row, Transaction, TransactionBehavior, params,
};
use sha2::{Digest, Sha256};
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
    #[error("invalid {kind} id in storage: {value}")]
    InvalidId { kind: &'static str, value: String },
    #[error("invalid stored {field} value: {value}")]
    InvalidEnumValue { field: &'static str, value: String },
    #[error("project {0} already exists")]
    ProjectAlreadyExists(ProjectId),
    #[error("project {0} does not exist")]
    ProjectNotFound(ProjectId),
    #[error("event {0} already exists")]
    EventAlreadyExists(Uuid),
    #[error("artifact {0} does not exist")]
    ArtifactNotFound(ArtifactId),
    #[error("blueprint revision {0} does not exist")]
    BlueprintNotFound(BlueprintRevisionId),
    #[error("blueprint revision {0} is not a draft")]
    BlueprintNotDraft(BlueprintRevisionId),
    #[error("info request {0} does not exist")]
    InfoRequestNotFound(InfoRequestId),
    #[error("info request {0} is not open")]
    InfoRequestNotOpen(InfoRequestId),
    #[error("info request answer cannot be empty")]
    EmptyInfoRequestAnswer,
    #[error("plan revision {0} does not exist")]
    PlanNotFound(PlanRevisionId),
    #[error("plan revision {0} is not a draft")]
    PlanNotDraft(PlanRevisionId),
    #[error("a plan revision is already active for this project")]
    PlanAlreadyActive,
    #[error("active work leases must be completed, released, or expired before replanning")]
    PlanLeaseActive,
    #[error("plan revision {0} is no longer active")]
    PlanNoLongerActive(PlanRevisionId),
    #[error("work item {0} has a stale task contract")]
    ContractStale(WorkItemId),
    #[error("publishing a plan requires a published blueprint")]
    PublishedBlueprintRequired,
    #[error("plan blueprint hash does not match the published blueprint")]
    BlueprintContentMismatch,
    #[error("work item {0} does not exist")]
    WorkItemNotFound(WorkItemId),
    #[error("work item {0} cannot be claimed in its current state")]
    WorkNotClaimable(WorkItemId),
    #[error("lease token is invalid for work item {0}")]
    LeaseInvalid(WorkItemId),
    #[error("lease for work item {0} has expired")]
    LeaseExpired(WorkItemId),
    #[error("work report contract hash does not match the active task contract")]
    ContractMismatch,
    #[error("work report was rejected: {0}")]
    ReportRejected(String),
    #[error("claim request id was already used: {0}")]
    DuplicateClaimRequest(String),
    #[error("claim request fields cannot be empty")]
    EmptyClaimRequest,
    #[error("release or blocker reason cannot be empty")]
    EmptyWorkReason,
    #[error("human completion note cannot be empty")]
    EmptyHumanWorkNote,
    #[error("work item {0} is not a human task")]
    HumanWorkRequired(WorkItemId),
    #[error("repository key {key} already exists for project {project_id}")]
    RepositoryKeyAlreadyExists { project_id: ProjectId, key: String },
    #[error("repository {0} does not exist in the project")]
    RepositoryNotFound(String),
    #[error("work item {0} has no verification snapshot")]
    VerificationNotConfigured(WorkItemId),
    #[error("work item {0} is already running verification")]
    VerificationInProgress(WorkItemId),
    #[error("work item {0} has no Review snapshot")]
    ReviewNotConfigured(WorkItemId),
    #[error("work item {0} is already running Review")]
    ReviewInProgress(WorkItemId),
    #[error("Review standard {0} does not exist in the project")]
    ReviewStandardNotFound(coldstart_core::ReviewStandardId),
    #[error("Review standard {0} has been deleted")]
    ReviewStandardDeleted(coldstart_core::ReviewStandardId),
    #[error("Review waiver reason cannot be empty")]
    EmptyReviewWaiverReason,
    #[error("invalid Review state: {0}")]
    ReviewInvalid(String),
    #[error("invalid verification command: {0}")]
    VerificationCommandInvalid(String),
    #[error("credential secret reference already exists: {0}")]
    CredentialSecretRefAlreadyExists(String),
    #[error("credential {0} does not belong to the requested project")]
    CredentialProjectMismatch(coldstart_core::CredentialId),
    #[error("project setting key cannot be empty")]
    EmptyProjectSettingKey,
    #[error("git sync branch and commit SHA cannot be empty")]
    EmptyGitSyncFields,
    #[error("git sync repository or work item does not belong to project {0}")]
    GitSyncScopeMismatch(ProjectId),
    #[error("invalid blueprint: {0}")]
    InvalidBlueprint(#[from] BlueprintValidationError),
    #[error("invalid plan: {0}")]
    InvalidPlan(#[from] PlanValidationError),
    #[error("invalid integration configuration: {0}")]
    InvalidIntegration(#[from] IntegrationValidationError),
    #[error("database schema version {found} is newer than supported version {supported}")]
    UnsupportedSchema { found: i64, supported: i64 },
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

    pub fn schema_version(&self) -> Result<i64, StorageError> {
        Ok(self
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    pub fn initialize_project(&mut self, project: &Project) -> Result<(), StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        if project_exists(&transaction, &project.id)? {
            return Err(StorageError::ProjectAlreadyExists(project.id));
        }

        transaction.execute(
            "INSERT INTO projects (id, name, created_at) VALUES (?1, ?2, ?3)",
            params![
                project.id.to_string(),
                project.name,
                project.created_at.to_rfc3339(),
            ],
        )?;
        insert_event(&transaction, &DomainEvent::project_initialized(project))?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn connection(&self) -> &Connection {
        &self.connection
    }

    pub(crate) fn connection_mut(&mut self) -> &mut Connection {
        &mut self.connection
    }

    pub fn load_project(
        &self,
        id: &ProjectId,
        root_path: PathBuf,
    ) -> Result<Option<Project>, StorageError> {
        let row = self
            .connection
            .query_row(
                "SELECT id, name, created_at FROM projects WHERE id = ?1",
                [id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;

        let Some((stored_id, name, created_at)) = row else {
            return Ok(None);
        };
        let created_at: DateTime<Utc> =
            DateTime::parse_from_rfc3339(&created_at)?.with_timezone(&Utc);

        Ok(Some(Project {
            id: stored_id.parse()?,
            name,
            root_path,
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

    pub fn insert_artifact(
        &mut self,
        artifact: &ArtifactMetadata,
        anchors: &[DocumentAnchor],
    ) -> Result<(), StorageError> {
        let anchors_json = serde_json::to_string(anchors)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !project_exists(&transaction, &artifact.project_id)? {
            return Err(StorageError::ProjectNotFound(artifact.project_id));
        }
        transaction.execute(
            "INSERT INTO artifacts (
                id, project_id, file_name, format, content_hash, text_hash, byte_size,
                import_source, anchors_json, imported_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                artifact.id.to_string(),
                artifact.project_id.to_string(),
                artifact.file_name,
                artifact.format.to_string(),
                artifact.content_hash,
                artifact.text_hash,
                artifact.byte_size as i64,
                artifact.import_source,
                anchors_json,
                artifact.imported_at.to_rfc3339(),
            ],
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                artifact.project_id,
                DomainEventKind::ArtifactImported {
                    artifact_id: artifact.id,
                    file_name: artifact.file_name.clone(),
                    content_hash: artifact.content_hash.clone(),
                },
            ),
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn list_artifacts(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<ArtifactMetadata>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, project_id, file_name, format, content_hash, text_hash, byte_size,
                    import_source, imported_at
             FROM artifacts WHERE project_id = ?1 ORDER BY imported_at ASC, id ASC",
        )?;
        let rows = statement.query_map([project_id.to_string()], read_artifact_row)?;
        let mut artifacts = Vec::new();
        for row in rows {
            artifacts.push(row?.into_metadata()?);
        }
        Ok(artifacts)
    }

    pub fn load_artifact_anchors(
        &self,
        artifact_id: &ArtifactId,
    ) -> Result<Vec<DocumentAnchor>, StorageError> {
        let anchors_json = self
            .connection
            .query_row(
                "SELECT anchors_json FROM artifacts WHERE id = ?1",
                [artifact_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or(StorageError::ArtifactNotFound(*artifact_id))?;
        Ok(serde_json::from_str(&anchors_json)?)
    }

    pub fn create_blueprint_revision(
        &mut self,
        project_id: &ProjectId,
        content: BlueprintContent,
        generation: BlueprintGeneration,
        info_requests: Vec<InfoRequestDraft>,
    ) -> Result<BlueprintRevision, StorageError> {
        validate_blueprint_structure(&content)?;
        let content_json = serde_json::to_string(&content)?;
        let content_hash = sha256_hex(content_json.as_bytes());
        let source_artifact_ids = content
            .source_references()
            .into_iter()
            .map(|source| source.artifact_id.to_string())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|value| parse_id(&value, "artifact"))
            .collect::<Result<Vec<ArtifactId>, _>>()?;
        let source_artifact_ids_json = serde_json::to_string(&source_artifact_ids)?;
        let created_at = Utc::now();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !project_exists(&transaction, project_id)? {
            return Err(StorageError::ProjectNotFound(*project_id));
        }
        let revision_number: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(revision_number), 0) + 1
             FROM blueprint_revisions WHERE project_id = ?1",
            [project_id.to_string()],
            |row| row.get(0),
        )?;
        let revision_id = BlueprintRevisionId::new();
        transaction.execute(
            "INSERT INTO blueprint_revisions (
                id, project_id, revision_number, status, content_json, content_hash,
                source_artifact_ids_json, generation_source, model_provider, model_name,
                prompt_version, created_at, published_at
             ) VALUES (?1, ?2, ?3, 'draft', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, NULL)",
            params![
                revision_id.to_string(),
                project_id.to_string(),
                revision_number,
                content_json,
                content_hash,
                source_artifact_ids_json,
                generation_source_str(generation.source),
                generation.model_provider,
                generation.model_name,
                generation.prompt_version,
                created_at.to_rfc3339(),
            ],
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::BlueprintDraftCreated {
                    revision_id,
                    revision_number: revision_number as u64,
                    generation_source: generation.source,
                },
            ),
        )?;

        for draft in info_requests {
            let fingerprint = draft.fingerprint();
            let exists: Option<String> = transaction
                .query_row(
                    "SELECT id FROM info_requests WHERE project_id = ?1 AND fingerprint = ?2",
                    params![project_id.to_string(), fingerprint],
                    |row| row.get(0),
                )
                .optional()?;
            if exists.is_some() {
                continue;
            }
            let request_id = InfoRequestId::new();
            transaction.execute(
                "INSERT INTO info_requests (
                    id, project_id, blueprint_revision_id, fingerprint, category, question,
                    reason, example, default_value, impact_scope, due_at, blocking, status,
                    answer, resolved_at, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'open',
                           NULL, NULL, ?13)",
                params![
                    request_id.to_string(),
                    project_id.to_string(),
                    revision_id.to_string(),
                    fingerprint,
                    draft.category.as_str(),
                    draft.question,
                    draft.reason,
                    draft.example,
                    draft.default_value,
                    draft.impact_scope,
                    draft.due_at.map(|date| date.to_rfc3339()),
                    if draft.blocking { 1 } else { 0 },
                    created_at.to_rfc3339(),
                ],
            )?;
            insert_event(
                &transaction,
                &DomainEvent::new(
                    *project_id,
                    DomainEventKind::InfoRequestCreated {
                        request_id,
                        question: draft.question,
                    },
                ),
            )?;
        }

        transaction.commit()?;
        Ok(BlueprintRevision {
            id: revision_id,
            project_id: *project_id,
            revision_number: revision_number as u64,
            status: BlueprintStatus::Draft,
            content,
            content_hash,
            source_artifact_ids,
            generation,
            created_at,
            published_at: None,
        })
    }

    pub fn load_blueprint(
        &self,
        revision_id: &BlueprintRevisionId,
    ) -> Result<Option<BlueprintRevision>, StorageError> {
        query_blueprint(&self.connection, revision_id)
    }

    pub fn latest_blueprint(
        &self,
        project_id: &ProjectId,
        status: BlueprintStatus,
    ) -> Result<Option<BlueprintRevision>, StorageError> {
        let row = self
            .connection
            .query_row(
                "SELECT id, project_id, revision_number, status, content_json, content_hash,
                        source_artifact_ids_json, generation_source, model_provider, model_name,
                        prompt_version, created_at, published_at
                 FROM blueprint_revisions
                 WHERE project_id = ?1 AND status = ?2
                 ORDER BY revision_number DESC LIMIT 1",
                params![project_id.to_string(), blueprint_status_str(status)],
                read_blueprint_row,
            )
            .optional()?;
        row.map(StoredBlueprintRow::into_revision).transpose()
    }

    pub fn publish_blueprint_revision(
        &mut self,
        project_id: &ProjectId,
        revision_id: &BlueprintRevisionId,
    ) -> Result<BlueprintRevision, StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut revision = query_blueprint(&transaction, revision_id)?
            .filter(|revision| revision.project_id == *project_id)
            .ok_or(StorageError::BlueprintNotFound(*revision_id))?;
        if revision.status != BlueprintStatus::Draft {
            return Err(StorageError::BlueprintNotDraft(*revision_id));
        }
        validate_blueprint_structure(&revision.content)?;
        let published_at = Utc::now();
        transaction.execute(
            "UPDATE blueprint_revisions SET status = 'superseded'
             WHERE project_id = ?1 AND id <> ?2 AND status IN ('published', 'draft')",
            params![project_id.to_string(), revision_id.to_string()],
        )?;
        transaction.execute(
            "UPDATE blueprint_revisions SET status = 'published', published_at = ?1
             WHERE id = ?2",
            params![published_at.to_rfc3339(), revision_id.to_string()],
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::BlueprintPublished {
                    revision_id: *revision_id,
                    revision_number: revision.revision_number,
                },
            ),
        )?;
        transaction.commit()?;
        revision.status = BlueprintStatus::Published;
        revision.published_at = Some(published_at);
        Ok(revision)
    }

    pub fn list_info_requests(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<InfoRequest>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, project_id, blueprint_revision_id, fingerprint, category, question,
                    reason, example, default_value, impact_scope, due_at, blocking, status,
                    answer, resolved_at, created_at
             FROM info_requests WHERE project_id = ?1
             ORDER BY CASE status WHEN 'open' THEN 0 ELSE 1 END, created_at ASC, id ASC",
        )?;
        let rows = statement.query_map([project_id.to_string()], read_info_request_row)?;
        let mut requests = Vec::new();
        for row in rows {
            requests.push(row?.into_info_request()?);
        }
        Ok(requests)
    }

    pub fn resolve_info_request(
        &mut self,
        project_id: &ProjectId,
        request_id: &InfoRequestId,
        answer: &str,
    ) -> Result<ProjectFact, StorageError> {
        let answer = answer.trim();
        if answer.is_empty() {
            return Err(StorageError::EmptyInfoRequestAnswer);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let request = query_info_request(&transaction, request_id)?
            .filter(|request| request.project_id == *project_id)
            .ok_or(StorageError::InfoRequestNotFound(*request_id))?;
        if request.status != InfoRequestStatus::Open {
            return Err(StorageError::InfoRequestNotOpen(*request_id));
        }
        let previous_version: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM project_facts
             WHERE project_id = ?1 AND fact_key = ?2",
            params![project_id.to_string(), request.fingerprint],
            |row| row.get(0),
        )?;
        let version = previous_version as u64 + 1;
        let fact_id = ProjectFactId::new();
        let resolved_at = Utc::now();
        transaction.execute(
            "INSERT INTO project_facts (
                id, project_id, fact_key, category, question, value, version,
                source_info_request_id, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                fact_id.to_string(),
                project_id.to_string(),
                request.fingerprint,
                request.category.as_str(),
                request.question,
                answer,
                version as i64,
                request_id.to_string(),
                resolved_at.to_rfc3339(),
            ],
        )?;
        transaction.execute(
            "UPDATE info_requests
             SET status = 'answered', answer = ?1, resolved_at = ?2
             WHERE id = ?3",
            params![answer, resolved_at.to_rfc3339(), request_id.to_string()],
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::InfoRequestResolved {
                    request_id: *request_id,
                    fact_id,
                    version,
                },
            ),
        )?;
        transaction.commit()?;
        Ok(ProjectFact {
            id: fact_id,
            project_id: *project_id,
            fact_key: request.fingerprint,
            category: request.category,
            question: request.question,
            value: answer.to_owned(),
            version,
            source_info_request_id: *request_id,
            created_at: resolved_at,
        })
    }

    pub fn list_project_facts(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<ProjectFact>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, project_id, fact_key, category, question, value, version,
                    source_info_request_id, created_at
             FROM project_facts WHERE project_id = ?1 ORDER BY created_at ASC, version ASC",
        )?;
        let rows = statement.query_map([project_id.to_string()], read_project_fact_row)?;
        let mut facts = Vec::new();
        for row in rows {
            facts.push(row?.into_project_fact()?);
        }
        Ok(facts)
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    format!("{:x}", digest.finalize())
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

pub(crate) fn parse_date(value: &str) -> Result<DateTime<Utc>, StorageError> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

fn parse_optional_date(value: Option<String>) -> Result<Option<DateTime<Utc>>, StorageError> {
    value.map(|value| parse_date(&value)).transpose()
}

fn generation_source_str(value: GenerationSource) -> &'static str {
    match value {
        GenerationSource::Model => "model",
        GenerationSource::Manual => "manual",
    }
}

fn parse_generation_source(value: &str) -> Result<GenerationSource, StorageError> {
    match value {
        "model" => Ok(GenerationSource::Model),
        "manual" => Ok(GenerationSource::Manual),
        _ => Err(StorageError::InvalidEnumValue {
            field: "generation_source",
            value: value.to_owned(),
        }),
    }
}

fn blueprint_status_str(value: BlueprintStatus) -> &'static str {
    match value {
        BlueprintStatus::Draft => "draft",
        BlueprintStatus::Published => "published",
        BlueprintStatus::Superseded => "superseded",
    }
}

fn parse_blueprint_status(value: &str) -> Result<BlueprintStatus, StorageError> {
    match value {
        "draft" => Ok(BlueprintStatus::Draft),
        "published" => Ok(BlueprintStatus::Published),
        "superseded" => Ok(BlueprintStatus::Superseded),
        _ => Err(StorageError::InvalidEnumValue {
            field: "blueprint status",
            value: value.to_owned(),
        }),
    }
}

fn parse_artifact_format(value: &str) -> Result<ArtifactFormat, StorageError> {
    ArtifactFormat::from_str(value).map_err(|_| StorageError::InvalidEnumValue {
        field: "artifact format",
        value: value.to_owned(),
    })
}

fn parse_info_request_category(value: &str) -> Result<InfoRequestCategory, StorageError> {
    value.parse().map_err(|_| StorageError::InvalidEnumValue {
        field: "info request category",
        value: value.to_owned(),
    })
}

fn parse_info_request_status(value: &str) -> Result<InfoRequestStatus, StorageError> {
    value.parse().map_err(|_| StorageError::InvalidEnumValue {
        field: "info request status",
        value: value.to_owned(),
    })
}

struct StoredArtifactRow {
    id: String,
    project_id: String,
    file_name: String,
    format: String,
    content_hash: String,
    text_hash: String,
    byte_size: i64,
    import_source: String,
    imported_at: String,
}

impl StoredArtifactRow {
    fn into_metadata(self) -> Result<ArtifactMetadata, StorageError> {
        Ok(ArtifactMetadata {
            id: parse_id(&self.id, "artifact")?,
            project_id: parse_id(&self.project_id, "project")?,
            file_name: self.file_name,
            format: parse_artifact_format(&self.format)?,
            content_hash: self.content_hash,
            text_hash: self.text_hash,
            byte_size: self.byte_size as u64,
            import_source: self.import_source,
            imported_at: parse_date(&self.imported_at)?,
        })
    }
}

fn read_artifact_row(row: &Row<'_>) -> rusqlite::Result<StoredArtifactRow> {
    Ok(StoredArtifactRow {
        id: row.get(0)?,
        project_id: row.get(1)?,
        file_name: row.get(2)?,
        format: row.get(3)?,
        content_hash: row.get(4)?,
        text_hash: row.get(5)?,
        byte_size: row.get(6)?,
        import_source: row.get(7)?,
        imported_at: row.get(8)?,
    })
}

struct StoredBlueprintRow {
    id: String,
    project_id: String,
    revision_number: i64,
    status: String,
    content_json: String,
    content_hash: String,
    source_artifact_ids_json: String,
    generation_source: String,
    model_provider: Option<String>,
    model_name: Option<String>,
    prompt_version: Option<String>,
    created_at: String,
    published_at: Option<String>,
}

impl StoredBlueprintRow {
    fn into_revision(self) -> Result<BlueprintRevision, StorageError> {
        let content: BlueprintContent = serde_json::from_str(&self.content_json)?;
        let source_values: Vec<String> = serde_json::from_str(&self.source_artifact_ids_json)?;
        let source_artifact_ids = source_values
            .iter()
            .map(|value| parse_id(value, "artifact"))
            .collect::<Result<Vec<ArtifactId>, _>>()?;
        Ok(BlueprintRevision {
            id: parse_id(&self.id, "blueprint revision")?,
            project_id: parse_id(&self.project_id, "project")?,
            revision_number: self.revision_number as u64,
            status: parse_blueprint_status(&self.status)?,
            content,
            content_hash: self.content_hash,
            source_artifact_ids,
            generation: BlueprintGeneration {
                source: parse_generation_source(&self.generation_source)?,
                model_provider: self.model_provider,
                model_name: self.model_name,
                prompt_version: self.prompt_version,
            },
            created_at: parse_date(&self.created_at)?,
            published_at: parse_optional_date(self.published_at)?,
        })
    }
}

fn read_blueprint_row(row: &Row<'_>) -> rusqlite::Result<StoredBlueprintRow> {
    Ok(StoredBlueprintRow {
        id: row.get(0)?,
        project_id: row.get(1)?,
        revision_number: row.get(2)?,
        status: row.get(3)?,
        content_json: row.get(4)?,
        content_hash: row.get(5)?,
        source_artifact_ids_json: row.get(6)?,
        generation_source: row.get(7)?,
        model_provider: row.get(8)?,
        model_name: row.get(9)?,
        prompt_version: row.get(10)?,
        created_at: row.get(11)?,
        published_at: row.get(12)?,
    })
}

struct StoredInfoRequestRow {
    id: String,
    project_id: String,
    blueprint_revision_id: String,
    fingerprint: String,
    category: String,
    question: String,
    reason: String,
    example: Option<String>,
    default_value: Option<String>,
    impact_scope: String,
    due_at: Option<String>,
    blocking: i64,
    status: String,
    answer: Option<String>,
    resolved_at: Option<String>,
    created_at: String,
}

impl StoredInfoRequestRow {
    fn into_info_request(self) -> Result<InfoRequest, StorageError> {
        Ok(InfoRequest {
            id: parse_id(&self.id, "info request")?,
            project_id: parse_id(&self.project_id, "project")?,
            blueprint_revision_id: parse_id(&self.blueprint_revision_id, "blueprint revision")?,
            fingerprint: self.fingerprint,
            category: parse_info_request_category(&self.category)?,
            question: self.question,
            reason: self.reason,
            example: self.example,
            default_value: self.default_value,
            impact_scope: self.impact_scope,
            due_at: parse_optional_date(self.due_at)?,
            blocking: self.blocking != 0,
            status: parse_info_request_status(&self.status)?,
            answer: self.answer,
            resolved_at: parse_optional_date(self.resolved_at)?,
            created_at: parse_date(&self.created_at)?,
        })
    }
}

fn read_info_request_row(row: &Row<'_>) -> rusqlite::Result<StoredInfoRequestRow> {
    Ok(StoredInfoRequestRow {
        id: row.get(0)?,
        project_id: row.get(1)?,
        blueprint_revision_id: row.get(2)?,
        fingerprint: row.get(3)?,
        category: row.get(4)?,
        question: row.get(5)?,
        reason: row.get(6)?,
        example: row.get(7)?,
        default_value: row.get(8)?,
        impact_scope: row.get(9)?,
        due_at: row.get(10)?,
        blocking: row.get(11)?,
        status: row.get(12)?,
        answer: row.get(13)?,
        resolved_at: row.get(14)?,
        created_at: row.get(15)?,
    })
}

struct StoredProjectFactRow {
    id: String,
    project_id: String,
    fact_key: String,
    category: String,
    question: String,
    value: String,
    version: i64,
    source_info_request_id: String,
    created_at: String,
}

impl StoredProjectFactRow {
    fn into_project_fact(self) -> Result<ProjectFact, StorageError> {
        Ok(ProjectFact {
            id: parse_id(&self.id, "project fact")?,
            project_id: parse_id(&self.project_id, "project")?,
            fact_key: self.fact_key,
            category: parse_info_request_category(&self.category)?,
            question: self.question,
            value: self.value,
            version: self.version as u64,
            source_info_request_id: parse_id(&self.source_info_request_id, "info request")?,
            created_at: parse_date(&self.created_at)?,
        })
    }
}

fn read_project_fact_row(row: &Row<'_>) -> rusqlite::Result<StoredProjectFactRow> {
    Ok(StoredProjectFactRow {
        id: row.get(0)?,
        project_id: row.get(1)?,
        fact_key: row.get(2)?,
        category: row.get(3)?,
        question: row.get(4)?,
        value: row.get(5)?,
        version: row.get(6)?,
        source_info_request_id: row.get(7)?,
        created_at: row.get(8)?,
    })
}

fn query_blueprint(
    connection: &Connection,
    revision_id: &BlueprintRevisionId,
) -> Result<Option<BlueprintRevision>, StorageError> {
    let row = connection
        .query_row(
            "SELECT id, project_id, revision_number, status, content_json, content_hash,
                    source_artifact_ids_json, generation_source, model_provider, model_name,
                    prompt_version, created_at, published_at
             FROM blueprint_revisions WHERE id = ?1",
            [revision_id.to_string()],
            read_blueprint_row,
        )
        .optional()?;
    row.map(StoredBlueprintRow::into_revision).transpose()
}

fn query_info_request(
    connection: &Connection,
    request_id: &InfoRequestId,
) -> Result<Option<InfoRequest>, StorageError> {
    let row = connection
        .query_row(
            "SELECT id, project_id, blueprint_revision_id, fingerprint, category, question,
                    reason, example, default_value, impact_scope, due_at, blocking, status,
                    answer, resolved_at, created_at
             FROM info_requests WHERE id = ?1",
            [request_id.to_string()],
            read_info_request_row,
        )
        .optional()?;
    row.map(StoredInfoRequestRow::into_info_request).transpose()
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

pub(crate) fn project_exists(
    transaction: &Transaction<'_>,
    id: &ProjectId,
) -> Result<bool, StorageError> {
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

pub(crate) fn insert_event(
    transaction: &Transaction<'_>,
    event: &DomainEvent,
) -> Result<(), StorageError> {
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

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn configures_wal_foreign_keys_and_busy_timeout() {
        let dir = TempDir::new().expect("temp directory must exist");
        let database = Database::open(&dir.path().join("state.db")).expect("database must open");
        let journal_mode: String = database
            .connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("journal mode must be readable");
        let foreign_keys: i64 = database
            .connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .expect("foreign key mode must be readable");
        let busy_timeout: i64 = database
            .connection
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .expect("busy timeout must be readable");

        assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
        assert_eq!(foreign_keys, 1);
        assert_eq!(busy_timeout, 5_000);
    }

    #[test]
    fn opening_the_current_schema_does_not_acquire_a_migration_write_lock() {
        let dir = TempDir::new().expect("temp directory must exist");
        let database_path = dir.path().join("state.db");
        drop(Database::open(&database_path).expect("database must initialize"));

        let blocker = Connection::open(&database_path).expect("blocker must open");
        blocker
            .execute_batch("BEGIN IMMEDIATE")
            .expect("blocker must acquire the write lock");
        let reopen_error = Database::open(&database_path)
            .err()
            .map(|error| error.to_string());
        blocker
            .execute_batch("ROLLBACK")
            .expect("blocker must release the write lock");

        assert_eq!(reopen_error, None);
    }
}
