use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Display,
    str::FromStr,
};

use chrono::{DateTime, Duration, Utc};
use coldstart_core::{
    ClaimFailure, ClaimResult, ClaimedWork, ContractState, DomainEvent, DomainEventKind, LeaseId,
    LeaseStatus, LeaseSummary, PlanContent, PlanPublishPreview, PlanRevision, PlanRevisionId,
    PlanStatus, PlanTaskDiff, ProjectId, ReviewConfigurationSnapshot, ReviewSnapshot,
    ReviewStandardSnapshot, ReviewStatus, TaskKind, TaskSpec, TaskSpecId, VerificationResult,
    VerificationSnapshot, VerificationStatus, WorkAuthorization, WorkBlockerRecord, WorkItem,
    WorkItemId, WorkItemStatus, WorkReportId, WorkReportInput, WorkReportRecord, WorkRun,
    WorkRunId, WorkRunStatus, inherited_work_status, sha256_hex, validate_work_report,
};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, TransactionBehavior, params};
use uuid::Uuid;

use crate::{
    Database, StorageError,
    database::insert_event,
    review_store::{
        applicable_review_standards_in, interrupt_running_review, review_configuration_in,
    },
    verification_store::{
        commands_for_repository, interrupt_running_verification, read_verification_fields,
        repository_id_by_key, verification_status_str,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewWorkBlocker {
    pub reason: String,
    pub needed_input: Vec<String>,
    pub proposal: Option<String>,
}

impl Database {
    pub fn create_plan_revision(
        &mut self,
        project_id: &ProjectId,
        blueprint_revision_id: coldstart_core::BlueprintRevisionId,
        content: PlanContent,
        now: DateTime<Utc>,
    ) -> Result<PlanRevision, StorageError> {
        content.validate()?;
        let content_json = serde_json::to_string(&content)?;
        let content_hash = sha256_hex(content_json.as_bytes());
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        let blueprint = transaction
            .query_row(
                "SELECT status, content_hash FROM blueprint_revisions
                 WHERE id = ?1 AND project_id = ?2",
                params![blueprint_revision_id.to_string(), project_id.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
            .ok_or(StorageError::PublishedBlueprintRequired)?;
        if blueprint.0 != "published" {
            return Err(StorageError::PublishedBlueprintRequired);
        }

        let revision_number: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(revision_number), 0) + 1 FROM plan_revisions WHERE project_id = ?1",
            [project_id.to_string()],
            |row| row.get(0),
        )?;
        let revision_id = PlanRevisionId::new();
        transaction.execute(
            "INSERT INTO plan_revisions (
                id, project_id, revision_number, blueprint_revision_id,
                blueprint_content_hash, template_version, status,
                content_json, content_hash, created_at, published_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'draft', ?7, ?8, ?9, NULL)",
            params![
                revision_id.to_string(),
                project_id.to_string(),
                revision_number,
                blueprint_revision_id.to_string(),
                blueprint.1,
                coldstart_core::PLAN_TEMPLATE_VERSION,
                content_json,
                content_hash,
                now.to_rfc3339(),
            ],
        )?;
        for task in &content.tasks {
            transaction.execute(
                "INSERT INTO task_specs (
                    id, plan_revision_id, task_key, contract_hash, kind, spec_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    TaskSpecId::new().to_string(),
                    revision_id.to_string(),
                    task.task_key,
                    task.contract_hash(),
                    task.kind.as_str(),
                    serde_json::to_string(task)?,
                ],
            )?;
        }
        insert_event(
            &transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::PlanDraftCreated {
                    revision_id,
                    revision_number: revision_number as u64,
                    task_count: content.tasks.len(),
                },
            ),
        )?;
        transaction.commit()?;
        Ok(PlanRevision {
            id: revision_id,
            project_id: *project_id,
            revision_number: revision_number as u64,
            blueprint_revision_id,
            blueprint_content_hash: blueprint.1,
            template_version: coldstart_core::PLAN_TEMPLATE_VERSION.to_owned(),
            status: PlanStatus::Draft,
            content,
            content_hash,
            created_at: now,
            published_at: None,
        })
    }

    pub fn load_plan(&self, id: &PlanRevisionId) -> Result<Option<PlanRevision>, StorageError> {
        query_plan(self.connection(), id)
    }

    pub fn latest_plan(
        &self,
        project_id: &ProjectId,
        status: PlanStatus,
    ) -> Result<Option<PlanRevision>, StorageError> {
        let row = self
            .connection()
            .query_row(
                "SELECT id, project_id, revision_number, blueprint_revision_id,
                        blueprint_content_hash, template_version, status,
                        content_json, content_hash, created_at, published_at
                 FROM plan_revisions
                 WHERE project_id = ?1 AND status = ?2
                 ORDER BY revision_number DESC LIMIT 1",
                params![project_id.to_string(), status.as_str()],
                read_plan_row,
            )
            .optional()?;
        row.map(StoredPlanRow::into_plan).transpose()
    }

    pub fn list_plans(&self, project_id: &ProjectId) -> Result<Vec<PlanRevision>, StorageError> {
        let mut statement = self.connection().prepare(
            "SELECT id, project_id, revision_number, blueprint_revision_id,
                    blueprint_content_hash, template_version, status,
                    content_json, content_hash, created_at, published_at
             FROM plan_revisions WHERE project_id = ?1 ORDER BY revision_number DESC",
        )?;
        let rows = statement.query_map([project_id.to_string()], read_plan_row)?;
        let mut plans = Vec::new();
        for row in rows {
            plans.push(row?.into_plan()?);
        }
        Ok(plans)
    }

    pub fn preview_plan_revision(
        &mut self,
        project_id: &ProjectId,
        plan_revision_id: &PlanRevisionId,
        now: DateTime<Utc>,
    ) -> Result<PlanPublishPreview, StorageError> {
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let plan = query_plan(&transaction, plan_revision_id)?
            .filter(|plan| plan.project_id == *project_id)
            .ok_or(StorageError::PlanNotFound(*plan_revision_id))?;
        if plan.status != PlanStatus::Draft {
            return Err(StorageError::PlanNotDraft(*plan_revision_id));
        }
        expire_active_leases(&transaction, project_id, now)?;
        let preview = build_publish_preview(&transaction, project_id, &plan)?;
        transaction.commit()?;
        Ok(preview)
    }

    pub fn publish_plan_revision(
        &mut self,
        project_id: &ProjectId,
        plan_revision_id: &PlanRevisionId,
        now: DateTime<Utc>,
    ) -> Result<PlanRevision, StorageError> {
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let plan = query_plan(&transaction, plan_revision_id)?
            .filter(|plan| plan.project_id == *project_id)
            .ok_or(StorageError::PlanNotFound(*plan_revision_id))?;
        if plan.status != PlanStatus::Draft {
            return Err(StorageError::PlanNotDraft(*plan_revision_id));
        }
        validate_plan_blueprint(&transaction, project_id, &plan)?;
        expire_active_leases(&transaction, project_id, now)?;
        let preview = build_publish_preview(&transaction, project_id, &plan)?;
        if preview.active_lease_count > 0 {
            return Err(StorageError::PlanLeaseActive);
        }

        let base = latest_published_plan(&transaction, project_id)?;
        let base_items = match &base {
            Some(base) => load_plan_items(&transaction, &base.id, project_id)?,
            None => Vec::new(),
        };
        let base_by_key = base_items
            .iter()
            .map(|(item, task)| (task.task_key.as_str(), (item, task)))
            .collect::<BTreeMap<_, _>>();

        if let Some(base) = &base {
            transaction.execute(
                "UPDATE plan_revisions SET status = 'superseded' WHERE id = ?1",
                [base.id.to_string()],
            )?;
            for (item, task) in &base_items {
                let replacement = plan.content.task(&task.task_key);
                let contract_changed = replacement
                    .is_some_and(|replacement| replacement.contract_hash() != task.contract_hash());
                if replacement.is_none() {
                    let next_status = if item.status == WorkItemStatus::Done {
                        WorkItemStatus::Done
                    } else {
                        WorkItemStatus::Cancelled
                    };
                    transaction.execute(
                        "UPDATE work_items
                         SET status = ?1, contract_state = 'stale', updated_at = ?2
                         WHERE id = ?3",
                        params![next_status.as_str(), now.to_rfc3339(), item.id.to_string()],
                    )?;
                } else if contract_changed {
                    transaction.execute(
                        "UPDATE work_items SET contract_state = 'stale', updated_at = ?1
                         WHERE id = ?2",
                        params![now.to_rfc3339(), item.id.to_string()],
                    )?;
                }
            }
            insert_event(
                &transaction,
                &DomainEvent::new(
                    *project_id,
                    DomainEventKind::PlanSuperseded {
                        revision_id: base.id,
                        superseded_by: plan.id,
                    },
                ),
            )?;
        }

        transaction.execute(
            "UPDATE plan_revisions SET status = 'published', published_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), plan_revision_id.to_string()],
        )?;
        for task in &plan.content.tasks {
            let spec_id: String = transaction.query_row(
                "SELECT id FROM task_specs WHERE plan_revision_id = ?1 AND task_key = ?2",
                params![plan_revision_id.to_string(), task.task_key],
                |row| row.get(0),
            )?;
            let previous = base_by_key.get(task.task_key.as_str()).copied();
            let (status, completed_at) = match previous {
                Some((item, previous_task)) => {
                    let contract_changed = previous_task.contract_hash() != task.contract_hash();
                    let status = inherited_work_status(item.status, contract_changed);
                    let completed_at = (status == WorkItemStatus::Done)
                        .then(|| item.completed_at.map(|value| value.to_rfc3339()))
                        .flatten();
                    (status, completed_at)
                }
                None => (WorkItemStatus::Draft, None),
            };
            transaction.execute(
                "INSERT INTO work_items (
                    id, project_id, plan_revision_id, task_spec_id, task_key,
                    status, contract_state, priority, created_at, updated_at, completed_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'current', ?7, ?8, ?8, ?9)",
                params![
                    coldstart_core::WorkItemId::new().to_string(),
                    project_id.to_string(),
                    plan_revision_id.to_string(),
                    spec_id,
                    task.task_key,
                    status.as_str(),
                    task.priority,
                    now.to_rfc3339(),
                    completed_at,
                ],
            )?;
        }
        insert_event(
            &transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::PlanPublished {
                    revision_id: *plan_revision_id,
                    revision_number: plan.revision_number,
                    task_count: plan.content.tasks.len(),
                },
            ),
        )?;
        refresh_work_items(&transaction, project_id, plan_revision_id, now)?;
        transaction.commit()?;
        self.load_plan(plan_revision_id)?
            .ok_or(StorageError::PlanNotFound(*plan_revision_id))
    }

    pub fn list_work_items(&self, project_id: &ProjectId) -> Result<Vec<WorkItem>, StorageError> {
        let mut statement = self.connection().prepare(
            "SELECT id, project_id, plan_revision_id, task_spec_id, task_key,
                    status, contract_state, priority, created_at, updated_at, completed_at
             FROM work_items WHERE project_id = ?1
             ORDER BY priority DESC, task_key ASC",
        )?;
        let rows = statement.query_map([project_id.to_string()], read_work_item_row)?;
        let mut work_items = Vec::new();
        for row in rows {
            work_items.push(row?.into_work_item()?);
        }
        Ok(work_items)
    }

    pub fn list_work_items_for_plan(
        &self,
        project_id: &ProjectId,
        plan_revision_id: &PlanRevisionId,
    ) -> Result<Vec<WorkItem>, StorageError> {
        let mut statement = self.connection().prepare(
            "SELECT id, project_id, plan_revision_id, task_spec_id, task_key,
                    status, contract_state, priority, created_at, updated_at, completed_at
             FROM work_items
             WHERE project_id = ?1 AND plan_revision_id = ?2
             ORDER BY priority DESC, task_key ASC",
        )?;
        let rows = statement.query_map(
            params![project_id.to_string(), plan_revision_id.to_string()],
            read_work_item_row,
        )?;
        let mut work_items = Vec::new();
        for row in rows {
            work_items.push(row?.into_work_item()?);
        }
        Ok(work_items)
    }

    pub fn load_work_item(&self, id: &WorkItemId) -> Result<Option<WorkItem>, StorageError> {
        let row = self
            .connection()
            .query_row(
                "SELECT id, project_id, plan_revision_id, task_spec_id, task_key,
                        status, contract_state, priority, created_at, updated_at, completed_at
                 FROM work_items WHERE id = ?1",
                [id.to_string()],
                read_work_item_row,
            )
            .optional()?;
        row.map(StoredWorkItemRow::into_work_item).transpose()
    }

    pub fn task_spec_for_work_item(
        &self,
        work_item_id: &WorkItemId,
    ) -> Result<Option<TaskSpec>, StorageError> {
        let value = self
            .connection()
            .query_row(
                "SELECT t.spec_json FROM task_specs t
                 JOIN work_items w ON w.task_spec_id = t.id WHERE w.id = ?1",
                [work_item_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        value
            .map(|value| serde_json::from_str(&value).map_err(StorageError::from))
            .transpose()
    }

    pub fn claim_work(
        &mut self,
        project_id: &ProjectId,
        agent_profile: &str,
        repository: Option<&str>,
        request_id: &str,
        now: DateTime<Utc>,
        lease_duration: Duration,
    ) -> Result<ClaimResult, StorageError> {
        self.claim_work_impl(
            project_id,
            agent_profile,
            repository,
            request_id,
            now,
            lease_duration,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn claim_work_with_review(
        &mut self,
        project_id: &ProjectId,
        agent_profile: &str,
        repository: Option<&str>,
        request_id: &str,
        now: DateTime<Utc>,
        lease_duration: Duration,
        review_base_commit: Option<&str>,
    ) -> Result<ClaimResult, StorageError> {
        self.claim_work_impl(
            project_id,
            agent_profile,
            repository,
            request_id,
            now,
            lease_duration,
            review_base_commit,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn claim_work_with_review_bases(
        &mut self,
        project_id: &ProjectId,
        agent_profile: &str,
        repository: Option<&str>,
        request_id: &str,
        now: DateTime<Utc>,
        lease_duration: Duration,
        review_base_commits: Option<&BTreeMap<(String, String), String>>,
    ) -> Result<ClaimResult, StorageError> {
        self.claim_work_impl(
            project_id,
            agent_profile,
            repository,
            request_id,
            now,
            lease_duration,
            None,
            review_base_commits,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn claim_work_impl(
        &mut self,
        project_id: &ProjectId,
        agent_profile: &str,
        repository: Option<&str>,
        request_id: &str,
        now: DateTime<Utc>,
        lease_duration: Duration,
        review_base_commit: Option<&str>,
        review_base_commits: Option<&BTreeMap<(String, String), String>>,
    ) -> Result<ClaimResult, StorageError> {
        if agent_profile.trim().is_empty() || request_id.trim().is_empty() {
            return Err(StorageError::EmptyClaimRequest);
        }
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let duplicate = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM work_runs WHERE claim_request_id = ?1)",
            [request_id.trim()],
            |row| row.get::<_, bool>(0),
        )?;
        if duplicate {
            return Err(StorageError::DuplicateClaimRequest(
                request_id.trim().to_owned(),
            ));
        }
        expire_active_leases(&transaction, project_id, now)?;

        let plan = latest_published_plan(&transaction, project_id)?;
        let Some(plan) = plan else {
            transaction.commit()?;
            return Ok(ClaimResult::Empty(ClaimFailure::NoActivePlan));
        };
        let items = load_plan_items(&transaction, &plan.id, project_id)?;
        let statuses = items
            .iter()
            .map(|(item, _)| (item.task_key.clone(), item.status))
            .collect::<BTreeMap<_, _>>();
        let locked_resources = active_resource_keys(&transaction, project_id)?;
        let mut candidate = None;
        let mut has_matching_agent_work = false;
        let mut has_human_work = false;

        for (item, task) in &items {
            if !matches!(
                item.status,
                WorkItemStatus::Draft | WorkItemStatus::Ready | WorkItemStatus::Reopened
            ) {
                continue;
            }
            if task.kind != TaskKind::AgentCoding {
                has_human_work = true;
                continue;
            }
            if repository
                .is_some_and(|repository| task.repository_key.as_deref() != Some(repository))
            {
                continue;
            }
            has_matching_agent_work = true;
            if !dependencies_done(task, &statuses) {
                continue;
            }
            if task
                .resource_keys
                .iter()
                .any(|resource| locked_resources.contains(resource))
            {
                continue;
            }
            candidate = Some((item, task));
            break;
        }

        let Some((item, task)) = candidate else {
            let failure = if items.iter().all(|(item, _)| {
                matches!(
                    item.status,
                    WorkItemStatus::Done | WorkItemStatus::Cancelled
                )
            }) {
                ClaimFailure::AllDone
            } else if has_human_work && !has_matching_agent_work {
                ClaimFailure::WaitingHuman
            } else if !has_matching_agent_work && repository.is_some() {
                ClaimFailure::NoMatchingRepository
            } else {
                ClaimFailure::WaitingDependencies
            };
            transaction.commit()?;
            return Ok(ClaimResult::Empty(failure));
        };

        let repository_key = task.repository_key.as_deref().unwrap_or_default();
        let repository_id = repository_id_by_key(&transaction, project_id, repository_key)?;
        let commands = commands_for_repository(&transaction, &repository_id)?;
        if commands.is_empty() {
            transaction.commit()?;
            return Ok(ClaimResult::Empty(ClaimFailure::VerificationNotConfigured));
        }
        let snapshot = VerificationSnapshot::new(repository_key.to_owned(), commands)
            .map_err(|error| StorageError::VerificationCommandInvalid(error.to_string()))?;
        let snapshot_json = serde_json::to_string(&snapshot)?;
        let snapshot_hash = snapshot.content_hash();

        let review_configuration = review_configuration_in(&transaction, project_id)?;
        let review_snapshot = match review_configuration {
            Some(configuration) if configuration.enabled => {
                if !configuration.has_valid_consent() {
                    transaction.commit()?;
                    return Ok(ClaimResult::Empty(ClaimFailure::ReviewConsentRequired));
                }
                if let Some(credential_id) = configuration.credential_id {
                    let credential_exists: bool = transaction.query_row(
                        "SELECT EXISTS(SELECT 1 FROM credentials
                         WHERE id = ?1 AND project_id = ?2)",
                        params![credential_id.to_string(), project_id.to_string()],
                        |row| row.get(0),
                    )?;
                    if !credential_exists {
                        transaction.commit()?;
                        return Ok(ClaimResult::Empty(ClaimFailure::ReviewCredentialMissing));
                    }
                }
                let base_ref = task.base_ref.as_deref().unwrap_or_default();
                let base_commit = review_base_commit.or_else(|| {
                    review_base_commits
                        .and_then(|commits| {
                            commits.get(&(repository_key.to_owned(), base_ref.to_owned()))
                        })
                        .map(String::as_str)
                });
                let Some(base_commit) = base_commit else {
                    transaction.commit()?;
                    return Ok(ClaimResult::Empty(ClaimFailure::ReviewNotConfigured));
                };
                let standards =
                    applicable_review_standards_in(&transaction, project_id, repository_key)?
                        .into_iter()
                        .map(|standard| ReviewStandardSnapshot {
                            standard_id: standard.id,
                            revision_id: standard.current_revision.id,
                            name: standard.name,
                            scope: standard.scope,
                            position: standard.position,
                            content_hash: standard.current_revision.content_hash,
                            character_count: standard.current_revision.character_count,
                        })
                        .collect();
                Some(
                    ReviewSnapshot::new(
                        repository_key,
                        base_commit,
                        ReviewConfigurationSnapshot::from_configuration(&configuration),
                        standards,
                    )
                    .map_err(|_| {
                        StorageError::ReviewInvalid(
                            "could not create a valid Review claim snapshot".to_owned(),
                        )
                    })?,
                )
            }
            _ => None,
        };
        let review_snapshot_json = review_snapshot
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let review_snapshot_hash = review_snapshot.as_ref().map(ReviewSnapshot::content_hash);

        let lease_token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let token_hash = sha256_hex(lease_token.as_bytes());
        let lease_id = LeaseId::new();
        let run_id = WorkRunId::new();
        let expires_at = now + lease_duration;
        transaction.execute(
            "INSERT INTO leases (
                id, work_item_id, agent_profile, token_hash, status,
                issued_at, expires_at, last_heartbeat_at, released_at
             ) VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?6, ?5, NULL)",
            params![
                lease_id.to_string(),
                item.id.to_string(),
                agent_profile.trim(),
                token_hash,
                now.to_rfc3339(),
                expires_at.to_rfc3339(),
            ],
        )?;
        transaction.execute(
            "INSERT INTO work_runs (
                id, work_item_id, lease_id, agent_profile, claim_request_id,
                status, verification_snapshot_json, verification_snapshot_hash,
                review_snapshot_json, review_snapshot_hash, started_at, ended_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, 'claimed', ?6, ?7, ?8, ?9, ?10, NULL)",
            params![
                run_id.to_string(),
                item.id.to_string(),
                lease_id.to_string(),
                agent_profile.trim(),
                request_id.trim(),
                snapshot_json,
                snapshot_hash,
                review_snapshot_json,
                review_snapshot_hash,
                now.to_rfc3339(),
            ],
        )?;
        transaction.execute(
            "UPDATE work_items SET status = 'claimed', updated_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), item.id.to_string()],
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::WorkClaimed {
                    work_item_id: item.id,
                    task_key: item.task_key.clone(),
                    lease_id,
                    agent_profile: agent_profile.trim().to_owned(),
                },
            ),
        )?;
        transaction.commit()?;

        let mut claimed_item = item.clone();
        claimed_item.status = WorkItemStatus::Claimed;
        claimed_item.updated_at = now;
        Ok(ClaimResult::Claimed(ClaimedWork {
            work_item: claimed_item,
            lease: LeaseSummary {
                id: lease_id,
                work_item_id: item.id,
                agent_profile: agent_profile.trim().to_owned(),
                status: LeaseStatus::Active,
                issued_at: now,
                expires_at,
                last_heartbeat_at: now,
                released_at: None,
            },
            run: WorkRun {
                id: run_id,
                work_item_id: item.id,
                lease_id,
                agent_profile: agent_profile.trim().to_owned(),
                claim_request_id: request_id.trim().to_owned(),
                status: WorkRunStatus::Claimed,
                verification_snapshot: Some(snapshot),
                verification_snapshot_hash: Some(snapshot_hash),
                review_snapshot: review_snapshot.clone(),
                review_snapshot_hash,
                started_at: now,
                ended_at: None,
            },
            lease_token,
        }))
    }

    pub fn authorize_work(
        &mut self,
        work_item_id: &WorkItemId,
        lease_token: &str,
        now: DateTime<Utc>,
        lease_duration: Duration,
    ) -> Result<WorkAuthorization, StorageError> {
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let authorization = authorize_in_tx(
            &transaction,
            work_item_id,
            lease_token,
            now,
            lease_duration,
            true,
        )?;
        transaction.commit()?;
        Ok(authorization)
    }

    pub fn authorize_work_readonly(
        &mut self,
        work_item_id: &WorkItemId,
        lease_token: &str,
        now: DateTime<Utc>,
    ) -> Result<WorkAuthorization, StorageError> {
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        let item = query_work_item(&transaction, work_item_id)?
            .ok_or(StorageError::WorkItemNotFound(*work_item_id))?;
        ensure_active_plan_item(&transaction, &item)?;
        if !matches!(
            item.status,
            WorkItemStatus::Claimed | WorkItemStatus::InProgress
        ) {
            return Err(StorageError::WorkNotClaimable(*work_item_id));
        }
        let task = query_task_spec_for_work_item(&transaction, work_item_id)?
            .ok_or(StorageError::WorkItemNotFound(*work_item_id))?;
        let lease = query_active_lease(&transaction, work_item_id)?.ok_or_else(|| {
            if query_had_expired_lease(&transaction, work_item_id).unwrap_or(false) {
                StorageError::LeaseExpired(*work_item_id)
            } else {
                StorageError::LeaseInvalid(*work_item_id)
            }
        })?;
        let supplied_hash = sha256_hex(lease_token.as_bytes());
        if !constant_time_eq(lease.token_hash.as_bytes(), supplied_hash.as_bytes()) {
            return Err(StorageError::LeaseInvalid(*work_item_id));
        }
        if lease.expires_at <= now {
            return Err(StorageError::LeaseExpired(*work_item_id));
        }
        let run = query_active_run_for_lease(&transaction, &lease.id)?
            .ok_or(StorageError::LeaseInvalid(*work_item_id))?;
        let lease_summary = LeaseSummary {
            id: lease.id,
            work_item_id: *work_item_id,
            agent_profile: lease.agent_profile,
            status: LeaseStatus::Active,
            issued_at: lease.issued_at,
            expires_at: lease.expires_at,
            last_heartbeat_at: lease.last_heartbeat_at,
            released_at: None,
        };
        transaction.commit()?;
        Ok(WorkAuthorization {
            work_item: item,
            task_spec: task,
            lease: lease_summary,
            run,
        })
    }

    pub fn expire_leases(
        &mut self,
        project_id: &ProjectId,
        now: DateTime<Utc>,
    ) -> Result<usize, StorageError> {
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let count = expire_active_leases(&transaction, project_id, now)?;
        transaction.commit()?;
        Ok(count)
    }

    pub fn heartbeat_work(
        &mut self,
        work_item_id: &WorkItemId,
        lease_token: &str,
        now: DateTime<Utc>,
        lease_duration: Duration,
    ) -> Result<LeaseSummary, StorageError> {
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let authorization = authorize_in_tx(
            &transaction,
            work_item_id,
            lease_token,
            now,
            lease_duration,
            true,
        )?;
        transaction.commit()?;
        Ok(authorization.lease)
    }

    pub fn release_work(
        &mut self,
        work_item_id: &WorkItemId,
        lease_token: &str,
        reason: &str,
        now: DateTime<Utc>,
        lease_duration: Duration,
    ) -> Result<WorkItem, StorageError> {
        if reason.trim().is_empty() {
            return Err(StorageError::EmptyWorkReason);
        }
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let authorization = authorize_in_tx(
            &transaction,
            work_item_id,
            lease_token,
            now,
            lease_duration,
            false,
        )?;
        transaction.execute(
            "UPDATE leases SET status = 'released', released_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), authorization.lease.id.to_string()],
        )?;
        transaction.execute(
            "UPDATE work_runs SET status = 'released', ended_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), authorization.run.id.to_string()],
        )?;
        transaction.execute(
            "UPDATE work_items SET status = 'ready', updated_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), work_item_id.to_string()],
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                authorization.work_item.project_id,
                DomainEventKind::WorkReleased {
                    work_item_id: *work_item_id,
                    lease_id: authorization.lease.id,
                    reason: reason.trim().to_owned(),
                },
            ),
        )?;
        refresh_work_items(
            &transaction,
            &authorization.work_item.project_id,
            &authorization.work_item.plan_revision_id,
            now,
        )?;
        transaction.commit()?;
        self.load_work_item(work_item_id)?
            .ok_or(StorageError::WorkItemNotFound(*work_item_id))
    }

    pub fn report_work_blocker(
        &mut self,
        work_item_id: &WorkItemId,
        lease_token: &str,
        blocker: NewWorkBlocker,
        now: DateTime<Utc>,
        lease_duration: Duration,
    ) -> Result<WorkBlockerRecord, StorageError> {
        let NewWorkBlocker {
            reason,
            needed_input,
            proposal,
        } = blocker;
        if reason.trim().is_empty() {
            return Err(StorageError::EmptyWorkReason);
        }
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let authorization = authorize_in_tx(
            &transaction,
            work_item_id,
            lease_token,
            now,
            lease_duration,
            false,
        )?;
        transaction.execute(
            "UPDATE leases SET status = 'released', released_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), authorization.lease.id.to_string()],
        )?;
        transaction.execute(
            "UPDATE work_runs SET status = 'blocked', ended_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), authorization.run.id.to_string()],
        )?;
        transaction.execute(
            "UPDATE work_items SET status = 'blocked', updated_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), work_item_id.to_string()],
        )?;
        transaction.execute(
            "INSERT INTO work_blockers (
                id, work_item_id, reason, needed_input_json, proposal, created_at, resolved_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)",
            params![
                Uuid::new_v4().to_string(),
                work_item_id.to_string(),
                reason.trim(),
                serde_json::to_string(&needed_input)?,
                proposal
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty()),
                now.to_rfc3339(),
            ],
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                authorization.work_item.project_id,
                DomainEventKind::WorkBlocked {
                    work_item_id: *work_item_id,
                    lease_id: authorization.lease.id,
                    reason: reason.trim().to_owned(),
                },
            ),
        )?;
        transaction.commit()?;
        Ok(WorkBlockerRecord {
            work_item_id: *work_item_id,
            reason: reason.trim().to_owned(),
            needed_input,
            proposal: proposal.filter(|value| !value.trim().is_empty()),
            created_at: now,
        })
    }

    pub fn submit_work_report(
        &mut self,
        work_item_id: &WorkItemId,
        lease_token: &str,
        report: WorkReportInput,
        now: DateTime<Utc>,
        lease_duration: Duration,
    ) -> Result<WorkReportRecord, StorageError> {
        if report.idempotency_key.trim().is_empty() {
            return Err(StorageError::ReportRejected(
                "report idempotency key cannot be empty".to_owned(),
            ));
        }
        let idempotency_hash = sha256_hex(report.idempotency_key.trim().as_bytes());
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(existing) =
            query_report_by_idempotency(&transaction, work_item_id, &idempotency_hash)?
        {
            transaction.commit()?;
            return Ok(existing);
        }

        let authorization = authorize_in_tx(
            &transaction,
            work_item_id,
            lease_token,
            now,
            lease_duration,
            false,
        )?;
        if let Err(error) = validate_work_report(&authorization.task_spec, &report) {
            let record = insert_work_report(
                &transaction,
                work_item_id,
                &authorization.run.id,
                WorkReportInsert {
                    report: &report,
                    idempotency_hash: &idempotency_hash,
                    accepted: false,
                    rejection_reasons: vec![error.to_string()],
                    verification_status: VerificationStatus::NotRequired,
                    verification: None,
                },
                now,
            )?;
            insert_event(
                &transaction,
                &DomainEvent::new(
                    authorization.work_item.project_id,
                    DomainEventKind::WorkReportSubmitted {
                        work_item_id: *work_item_id,
                        report_id: record.id,
                        accepted: false,
                    },
                ),
            )?;
            transaction.commit()?;
            return Err(StorageError::ReportRejected(error.to_string()));
        }

        let record = insert_work_report(
            &transaction,
            work_item_id,
            &authorization.run.id,
            WorkReportInsert {
                report: &report,
                idempotency_hash: &idempotency_hash,
                accepted: true,
                rejection_reasons: vec![],
                verification_status: VerificationStatus::NotRequired,
                verification: None,
            },
            now,
        )?;
        transaction.execute(
            "UPDATE leases SET status = 'consumed', released_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), authorization.lease.id.to_string()],
        )?;
        transaction.execute(
            "UPDATE work_runs SET status = 'completed', ended_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), authorization.run.id.to_string()],
        )?;
        transaction.execute(
            "UPDATE work_items
             SET status = 'done', updated_at = ?1, completed_at = ?1
             WHERE id = ?2",
            params![now.to_rfc3339(), work_item_id.to_string()],
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                authorization.work_item.project_id,
                DomainEventKind::WorkReportSubmitted {
                    work_item_id: *work_item_id,
                    report_id: record.id,
                    accepted: true,
                },
            ),
        )?;
        insert_event(
            &transaction,
            &DomainEvent::new(
                authorization.work_item.project_id,
                DomainEventKind::WorkCompleted {
                    work_item_id: *work_item_id,
                    task_key: authorization.work_item.task_key.clone(),
                },
            ),
        )?;
        refresh_work_items(
            &transaction,
            &authorization.work_item.project_id,
            &authorization.work_item.plan_revision_id,
            now,
        )?;
        transaction.commit()?;
        Ok(record)
    }

    pub fn complete_human_work(
        &mut self,
        project_id: &ProjectId,
        work_item_id: &WorkItemId,
        note: &str,
        accepted: bool,
        now: DateTime<Utc>,
    ) -> Result<WorkItem, StorageError> {
        if note.trim().is_empty() {
            return Err(StorageError::EmptyHumanWorkNote);
        }
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let item = query_work_item(&transaction, work_item_id)?
            .filter(|item| item.project_id == *project_id)
            .ok_or(StorageError::WorkItemNotFound(*work_item_id))?;
        ensure_active_plan_item(&transaction, &item)?;
        let task = query_task_spec_for_work_item(&transaction, work_item_id)?
            .ok_or(StorageError::WorkItemNotFound(*work_item_id))?;
        if !matches!(task.kind, TaskKind::HumanExternal | TaskKind::ApprovalGate) {
            return Err(StorageError::HumanWorkRequired(*work_item_id));
        }
        if !matches!(
            item.status,
            WorkItemStatus::Draft | WorkItemStatus::Ready | WorkItemStatus::Blocked
        ) {
            return Err(StorageError::WorkNotClaimable(*work_item_id));
        }

        let next_status = if accepted {
            WorkItemStatus::Done
        } else {
            WorkItemStatus::Blocked
        };
        transaction.execute(
            "UPDATE work_items SET status = ?1, updated_at = ?2, completed_at = ?3 WHERE id = ?4",
            params![
                next_status.as_str(),
                now.to_rfc3339(),
                accepted.then(|| now.to_rfc3339()),
                work_item_id.to_string(),
            ],
        )?;
        if !accepted {
            transaction.execute(
                "INSERT INTO work_blockers (
                    id, work_item_id, reason, needed_input_json, proposal, created_at, resolved_at
                 ) VALUES (?1, ?2, ?3, '[]', NULL, ?4, NULL)",
                params![
                    Uuid::new_v4().to_string(),
                    work_item_id.to_string(),
                    note.trim(),
                    now.to_rfc3339(),
                ],
            )?;
        } else {
            transaction.execute(
                "UPDATE work_blockers SET resolved_at = ?1
                 WHERE work_item_id = ?2 AND resolved_at IS NULL",
                params![now.to_rfc3339(), work_item_id.to_string()],
            )?;
        }
        insert_event(
            &transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::HumanWorkCompleted {
                    work_item_id: *work_item_id,
                    accepted,
                },
            ),
        )?;
        refresh_work_items(&transaction, project_id, &item.plan_revision_id, now)?;
        transaction.commit()?;
        self.load_work_item(work_item_id)?
            .ok_or(StorageError::WorkItemNotFound(*work_item_id))
    }

    pub fn list_leases(&self, project_id: &ProjectId) -> Result<Vec<LeaseSummary>, StorageError> {
        let mut statement = self.connection().prepare(
            "SELECT l.id, l.work_item_id, l.agent_profile, l.status, l.issued_at,
                    l.expires_at, l.last_heartbeat_at, l.released_at
             FROM leases l JOIN work_items w ON w.id = l.work_item_id
             WHERE w.project_id = ?1 ORDER BY l.issued_at DESC LIMIT 200",
        )?;
        let rows = statement.query_map([project_id.to_string()], read_lease_row)?;
        let mut leases = Vec::new();
        for row in rows {
            leases.push(row?.into_lease_summary()?);
        }
        Ok(leases)
    }

    pub fn list_work_runs(&self, project_id: &ProjectId) -> Result<Vec<WorkRun>, StorageError> {
        let mut statement = self.connection().prepare(
            "SELECT r.id, r.work_item_id, r.lease_id, r.agent_profile,
                    r.claim_request_id, r.status, r.verification_snapshot_json,
                    r.verification_snapshot_hash, r.review_snapshot_json,
                    r.review_snapshot_hash, r.started_at, r.ended_at
             FROM work_runs r JOIN work_items w ON w.id = r.work_item_id
             WHERE w.project_id = ?1 ORDER BY r.started_at DESC LIMIT 200",
        )?;
        let rows = statement.query_map([project_id.to_string()], read_run_row)?;
        let mut runs = Vec::new();
        for row in rows {
            runs.push(row?.into_run()?);
        }
        Ok(runs)
    }

    pub fn list_work_reports(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<WorkReportRecord>, StorageError> {
        let mut statement = self.connection().prepare(
            "SELECT r.id, r.work_item_id, r.work_run_id, r.contract_hash,
                    r.report_json, r.report_hash, r.accepted,
                    r.rejection_reasons_json, r.created_at,
                    r.verification_status, r.verification_json,
                    r.verification_started_at, r.verification_finished_at,
                    r.review_status, r.review_json, r.review_started_at,
                    r.review_finished_at, r.review_waiver_json
             FROM work_reports r JOIN work_items w ON w.id = r.work_item_id
             WHERE w.project_id = ?1 ORDER BY r.created_at DESC LIMIT 200",
        )?;
        let rows = statement.query_map([project_id.to_string()], read_report_row)?;
        let mut reports = Vec::new();
        for row in rows {
            reports.push(row?.into_record()?);
        }
        Ok(reports)
    }

    pub fn list_unresolved_blockers(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<WorkBlockerRecord>, StorageError> {
        let mut statement = self.connection().prepare(
            "SELECT b.work_item_id, b.reason, b.needed_input_json, b.proposal, b.created_at
             FROM work_blockers b JOIN work_items w ON w.id = b.work_item_id
             WHERE w.project_id = ?1 AND b.resolved_at IS NULL
             ORDER BY b.created_at DESC",
        )?;
        let rows = statement.query_map([project_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        let mut blockers = Vec::new();
        for row in rows {
            let (work_item_id, reason, needed_input, proposal, created_at) = row?;
            blockers.push(WorkBlockerRecord {
                work_item_id: parse_id(&work_item_id, "work item")?,
                reason,
                needed_input: serde_json::from_str(&needed_input)?,
                proposal,
                created_at: parse_date(&created_at)?,
            });
        }
        Ok(blockers)
    }
}

fn expire_active_leases(
    transaction: &Transaction<'_>,
    project_id: &ProjectId,
    now: DateTime<Utc>,
) -> Result<usize, StorageError> {
    let mut statement = transaction.prepare(
        "SELECT l.id, l.work_item_id, w.status
         FROM leases l JOIN work_items w ON w.id = l.work_item_id
         WHERE w.project_id = ?1 AND l.status = 'active' AND l.expires_at <= ?2",
    )?;
    let rows = statement.query_map(params![project_id.to_string(), now.to_rfc3339()], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut expired = Vec::new();
    for row in rows {
        expired.push(row?);
    }
    drop(statement);
    let count = expired.len();
    for (lease_id, work_item_id, status) in expired {
        let lease_id: LeaseId = parse_id(&lease_id, "lease")?;
        let work_item_id: WorkItemId = parse_id(&work_item_id, "work item")?;
        let status: WorkItemStatus = parse_enum(&status, "work item status")?;
        transaction.execute(
            "UPDATE leases SET status = 'expired', released_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), lease_id.to_string()],
        )?;
        transaction.execute(
            "UPDATE work_runs SET status = 'expired', ended_at = ?1
             WHERE lease_id = ?2 AND ended_at IS NULL",
            params![now.to_rfc3339(), lease_id.to_string()],
        )?;
        interrupt_running_verification(transaction, &work_item_id, now)?;
        interrupt_running_review(transaction, &work_item_id, now)?;
        if matches!(
            status,
            WorkItemStatus::Claimed | WorkItemStatus::InProgress | WorkItemStatus::Submitted
        ) {
            transaction.execute(
                "UPDATE work_items SET status = 'ready', updated_at = ?1 WHERE id = ?2",
                params![now.to_rfc3339(), work_item_id.to_string()],
            )?;
        }
        insert_event(
            transaction,
            &DomainEvent::new(
                *project_id,
                DomainEventKind::LeaseExpired {
                    work_item_id,
                    lease_id,
                },
            ),
        )?;
    }
    Ok(count)
}

fn latest_published_plan(
    transaction: &Transaction<'_>,
    project_id: &ProjectId,
) -> Result<Option<PlanRevision>, StorageError> {
    let id = transaction
        .query_row(
            "SELECT id FROM plan_revisions
             WHERE project_id = ?1 AND status = 'published'
             ORDER BY revision_number DESC LIMIT 1",
            [project_id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(id) = id else {
        return Ok(None);
    };
    query_plan(transaction, &parse_id(&id, "plan revision")?)
}

fn validate_plan_blueprint(
    transaction: &Transaction<'_>,
    project_id: &ProjectId,
    plan: &PlanRevision,
) -> Result<(), StorageError> {
    let blueprint = transaction
        .query_row(
            "SELECT status, content_hash FROM blueprint_revisions
             WHERE id = ?1 AND project_id = ?2",
            params![
                plan.blueprint_revision_id.to_string(),
                project_id.to_string()
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
        .ok_or(StorageError::PublishedBlueprintRequired)?;
    if blueprint.0 != "published" {
        return Err(StorageError::PublishedBlueprintRequired);
    }
    if blueprint.1 != plan.blueprint_content_hash {
        return Err(StorageError::BlueprintContentMismatch);
    }
    Ok(())
}

fn build_publish_preview(
    transaction: &Transaction<'_>,
    project_id: &ProjectId,
    candidate: &PlanRevision,
) -> Result<PlanPublishPreview, StorageError> {
    let base = latest_published_plan(transaction, project_id)?;
    let base_items = match &base {
        Some(base) => load_plan_items(transaction, &base.id, project_id)?,
        None => Vec::new(),
    };
    let base_by_key = base_items
        .iter()
        .map(|(item, task)| (task.task_key.as_str(), (item, task)))
        .collect::<BTreeMap<_, _>>();
    let mut added = Vec::new();
    let mut changed = Vec::new();
    let mut unchanged = Vec::new();
    let mut carried_completed = 0;
    let mut reopened = 0;
    for task in &candidate.content.tasks {
        let previous = base_by_key.get(task.task_key.as_str()).copied();
        let contract_changed = previous.is_some_and(|(_, previous_task)| {
            previous_task.contract_hash() != task.contract_hash()
        });
        let previous_status = previous.map(|(item, _)| item.status);
        let next_status = previous.map_or(WorkItemStatus::Draft, |(item, _)| {
            inherited_work_status(item.status, contract_changed)
        });
        if previous_status == Some(WorkItemStatus::Done) && !contract_changed {
            carried_completed += 1;
        }
        if next_status == WorkItemStatus::Reopened {
            reopened += 1;
        }
        let diff = PlanTaskDiff {
            task_key: task.task_key.clone(),
            title: task.title.clone(),
            previous_status,
            next_status: Some(next_status),
            contract_changed,
        };
        if previous.is_none() {
            added.push(diff);
        } else if contract_changed {
            changed.push(diff);
        } else {
            unchanged.push(diff);
        }
    }

    let candidate_keys = candidate
        .content
        .tasks
        .iter()
        .map(|task| task.task_key.as_str())
        .collect::<BTreeSet<_>>();
    let mut removed = Vec::new();
    let mut cancelled = 0;
    for (item, task) in &base_items {
        if candidate_keys.contains(task.task_key.as_str()) {
            continue;
        }
        let next_status = if item.status == WorkItemStatus::Done {
            WorkItemStatus::Done
        } else {
            cancelled += 1;
            WorkItemStatus::Cancelled
        };
        removed.push(PlanTaskDiff {
            task_key: task.task_key.clone(),
            title: task.title.clone(),
            previous_status: Some(item.status),
            next_status: Some(next_status),
            contract_changed: false,
        });
    }

    let active_lease_count: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM leases l
         JOIN work_items w ON w.id = l.work_item_id
         WHERE w.project_id = ?1 AND l.status = 'active'",
        [project_id.to_string()],
        |row| row.get(0),
    )?;
    let active_lease_count = usize::try_from(active_lease_count).unwrap_or(usize::MAX);
    let blockers = if active_lease_count == 0 {
        Vec::new()
    } else {
        vec![format!(
            "仍有 {active_lease_count} 个活动租约，必须完成、释放或等待过期后才能替换活动计划"
        )]
    };
    Ok(PlanPublishPreview {
        base_revision_id: base.as_ref().map(|base| base.id),
        base_revision_number: base.as_ref().map(|base| base.revision_number),
        candidate_revision_id: candidate.id,
        candidate_revision_number: candidate.revision_number,
        added,
        changed,
        unchanged,
        removed,
        carried_completed,
        reopened,
        cancelled,
        active_lease_count,
        publishable: blockers.is_empty(),
        blockers,
    })
}

fn ensure_active_plan_item(
    transaction: &Transaction<'_>,
    item: &WorkItem,
) -> Result<(), StorageError> {
    let active_plan_id = latest_published_plan(transaction, &item.project_id)?.map(|plan| plan.id);
    if active_plan_id != Some(item.plan_revision_id) {
        return Err(StorageError::PlanNoLongerActive(item.plan_revision_id));
    }
    if item.contract_state != ContractState::Current {
        return Err(StorageError::ContractStale(item.id));
    }
    Ok(())
}

fn load_plan_items(
    transaction: &Transaction<'_>,
    plan_revision_id: &PlanRevisionId,
    project_id: &ProjectId,
) -> Result<Vec<(WorkItem, TaskSpec)>, StorageError> {
    let mut statement = transaction.prepare(
        "SELECT w.id, w.project_id, w.plan_revision_id, w.task_spec_id, w.task_key,
                w.status, w.contract_state, w.priority, w.created_at, w.updated_at,
                w.completed_at, t.spec_json
         FROM work_items w JOIN task_specs t ON t.id = w.task_spec_id
         WHERE w.plan_revision_id = ?1 AND w.project_id = ?2
         ORDER BY w.priority DESC, w.task_key ASC",
    )?;
    let rows = statement.query_map(
        params![plan_revision_id.to_string(), project_id.to_string()],
        |row| Ok((read_work_item_row(row)?, row.get::<_, String>(11)?)),
    )?;
    let mut items = Vec::new();
    for row in rows {
        let (item, spec_json) = row?;
        items.push((item.into_work_item()?, serde_json::from_str(&spec_json)?));
    }
    Ok(items)
}

fn active_resource_keys(
    transaction: &Transaction<'_>,
    project_id: &ProjectId,
) -> Result<BTreeSet<String>, StorageError> {
    let mut statement = transaction.prepare(
        "SELECT t.spec_json FROM leases l
         JOIN work_items w ON w.id = l.work_item_id
         JOIN task_specs t ON t.id = w.task_spec_id
         WHERE w.project_id = ?1 AND l.status = 'active'",
    )?;
    let rows = statement.query_map([project_id.to_string()], |row| row.get::<_, String>(0))?;
    let mut resources = BTreeSet::new();
    for row in rows {
        let task: TaskSpec = serde_json::from_str(&row?)?;
        resources.extend(task.resource_keys);
    }
    Ok(resources)
}

pub(crate) fn authorize_in_tx(
    transaction: &Transaction<'_>,
    work_item_id: &WorkItemId,
    lease_token: &str,
    now: DateTime<Utc>,
    lease_duration: Duration,
    record_heartbeat: bool,
) -> Result<WorkAuthorization, StorageError> {
    let initial_item = query_work_item(transaction, work_item_id)?
        .ok_or(StorageError::WorkItemNotFound(*work_item_id))?;
    expire_active_leases(transaction, &initial_item.project_id, now)?;
    let item = query_work_item(transaction, work_item_id)?
        .ok_or(StorageError::WorkItemNotFound(*work_item_id))?;
    ensure_active_plan_item(transaction, &item)?;
    if !matches!(
        item.status,
        WorkItemStatus::Claimed | WorkItemStatus::InProgress
    ) {
        return Err(StorageError::WorkNotClaimable(*work_item_id));
    }
    let task = query_task_spec_for_work_item(transaction, work_item_id)?
        .ok_or(StorageError::WorkItemNotFound(*work_item_id))?;
    let lease = query_active_lease(transaction, work_item_id)?.ok_or_else(|| {
        if query_had_expired_lease(transaction, work_item_id).unwrap_or(false) {
            StorageError::LeaseExpired(*work_item_id)
        } else {
            StorageError::LeaseInvalid(*work_item_id)
        }
    })?;
    let supplied_hash = sha256_hex(lease_token.as_bytes());
    if !constant_time_eq(lease.token_hash.as_bytes(), supplied_hash.as_bytes()) {
        return Err(StorageError::LeaseInvalid(*work_item_id));
    }
    if lease.expires_at <= now {
        return Err(StorageError::LeaseExpired(*work_item_id));
    }
    let mut run = query_active_run_for_lease(transaction, &lease.id)?
        .ok_or(StorageError::LeaseInvalid(*work_item_id))?;
    let expires_at = now + lease_duration;
    transaction.execute(
        "UPDATE leases SET last_heartbeat_at = ?1, expires_at = ?2 WHERE id = ?3",
        params![
            now.to_rfc3339(),
            expires_at.to_rfc3339(),
            lease.id.to_string(),
        ],
    )?;

    let mut updated_item = item.clone();
    if item.status == WorkItemStatus::Claimed {
        transaction.execute(
            "UPDATE work_items SET status = 'in_progress', updated_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), work_item_id.to_string()],
        )?;
        transaction.execute(
            "UPDATE work_runs SET status = 'running' WHERE id = ?1",
            [run.id.to_string()],
        )?;
        updated_item.status = WorkItemStatus::InProgress;
        updated_item.updated_at = now;
        run.status = WorkRunStatus::Running;
        insert_event(
            transaction,
            &DomainEvent::new(
                item.project_id,
                DomainEventKind::WorkStarted {
                    work_item_id: *work_item_id,
                },
            ),
        )?;
    }
    if record_heartbeat {
        insert_event(
            transaction,
            &DomainEvent::new(
                item.project_id,
                DomainEventKind::WorkHeartbeat {
                    work_item_id: *work_item_id,
                    lease_id: lease.id,
                    expires_at,
                },
            ),
        )?;
    }
    let lease_summary = LeaseSummary {
        id: lease.id,
        work_item_id: *work_item_id,
        agent_profile: lease.agent_profile,
        status: LeaseStatus::Active,
        issued_at: lease.issued_at,
        expires_at,
        last_heartbeat_at: now,
        released_at: None,
    };
    Ok(WorkAuthorization {
        work_item: updated_item,
        task_spec: task,
        lease: lease_summary,
        run,
    })
}

pub(crate) struct WorkReportInsert<'a> {
    pub report: &'a WorkReportInput,
    pub idempotency_hash: &'a str,
    pub accepted: bool,
    pub rejection_reasons: Vec<String>,
    pub verification_status: VerificationStatus,
    pub verification: Option<VerificationResult>,
}

pub(crate) fn insert_work_report(
    transaction: &Transaction<'_>,
    work_item_id: &WorkItemId,
    run_id: &WorkRunId,
    input: WorkReportInsert<'_>,
    now: DateTime<Utc>,
) -> Result<WorkReportRecord, StorageError> {
    let WorkReportInsert {
        report,
        idempotency_hash,
        accepted,
        rejection_reasons,
        verification_status,
        verification,
    } = input;
    let report_id = WorkReportId::new();
    let report_json = serde_json::to_string(report)?;
    let report_hash = sha256_hex(report_json.as_bytes());
    transaction.execute(
        "INSERT INTO work_reports (
            id, work_item_id, work_run_id, contract_hash, idempotency_key_hash,
            report_json, report_hash, accepted, rejection_reasons_json, created_at,
            verification_status, verification_json, verification_started_at,
            verification_finished_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            report_id.to_string(),
            work_item_id.to_string(),
            run_id.to_string(),
            report.contract_hash,
            idempotency_hash,
            report_json,
            report_hash,
            i64::from(accepted),
            serde_json::to_string(&rejection_reasons)?,
            now.to_rfc3339(),
            verification_status_str(verification_status),
            verification
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?,
            verification
                .as_ref()
                .map(|result| result.started_at.to_rfc3339()),
            verification
                .as_ref()
                .and_then(|result| result.finished_at)
                .map(|value| value.to_rfc3339()),
        ],
    )?;
    Ok(WorkReportRecord {
        id: report_id,
        work_item_id: *work_item_id,
        work_run_id: *run_id,
        contract_hash: report.contract_hash.clone(),
        report: report.clone(),
        report_hash,
        accepted,
        rejection_reasons,
        verification_status,
        verification,
        review_status: ReviewStatus::NotRequired,
        review: None,
        review_waiver: None,
        created_at: now,
    })
}

fn query_work_item(
    connection: &Connection,
    id: &WorkItemId,
) -> Result<Option<WorkItem>, StorageError> {
    let row = connection
        .query_row(
            "SELECT id, project_id, plan_revision_id, task_spec_id, task_key,
                    status, contract_state, priority, created_at, updated_at, completed_at
             FROM work_items WHERE id = ?1",
            [id.to_string()],
            read_work_item_row,
        )
        .optional()?;
    row.map(StoredWorkItemRow::into_work_item).transpose()
}

fn query_task_spec_for_work_item(
    connection: &Connection,
    work_item_id: &WorkItemId,
) -> Result<Option<TaskSpec>, StorageError> {
    let value = connection
        .query_row(
            "SELECT t.spec_json FROM task_specs t
             JOIN work_items w ON w.task_spec_id = t.id WHERE w.id = ?1",
            [work_item_id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    value
        .map(|value| serde_json::from_str(&value).map_err(StorageError::from))
        .transpose()
}

#[derive(Debug)]
struct StoredLeaseRow {
    id: String,
    work_item_id: String,
    agent_profile: String,
    status: String,
    issued_at: String,
    expires_at: String,
    last_heartbeat_at: String,
    released_at: Option<String>,
}

#[derive(Debug)]
struct ActiveLeaseRow {
    id: LeaseId,
    agent_profile: String,
    token_hash: String,
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    last_heartbeat_at: DateTime<Utc>,
}

impl StoredLeaseRow {
    fn into_lease_summary(self) -> Result<LeaseSummary, StorageError> {
        Ok(LeaseSummary {
            id: parse_id(&self.id, "lease")?,
            work_item_id: parse_id(&self.work_item_id, "work item")?,
            agent_profile: self.agent_profile,
            status: parse_enum(&self.status, "lease status")?,
            issued_at: parse_date(&self.issued_at)?,
            expires_at: parse_date(&self.expires_at)?,
            last_heartbeat_at: parse_date(&self.last_heartbeat_at)?,
            released_at: parse_optional_date(self.released_at)?,
        })
    }
}

fn read_lease_row(row: &Row<'_>) -> rusqlite::Result<StoredLeaseRow> {
    Ok(StoredLeaseRow {
        id: row.get(0)?,
        work_item_id: row.get(1)?,
        agent_profile: row.get(2)?,
        status: row.get(3)?,
        issued_at: row.get(4)?,
        expires_at: row.get(5)?,
        last_heartbeat_at: row.get(6)?,
        released_at: row.get(7)?,
    })
}

fn query_active_lease(
    connection: &Connection,
    work_item_id: &WorkItemId,
) -> Result<Option<ActiveLeaseRow>, StorageError> {
    let row = connection
        .query_row(
            "SELECT id, work_item_id, agent_profile, token_hash, issued_at, expires_at,
                    last_heartbeat_at
             FROM leases WHERE work_item_id = ?1 AND status = 'active'",
            [work_item_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .optional()?;
    row.map(
        |(
            id,
            _work_item_id,
            agent_profile,
            token_hash,
            issued_at,
            expires_at,
            last_heartbeat_at,
        )| {
            Ok(ActiveLeaseRow {
                id: parse_id(&id, "lease")?,
                agent_profile,
                token_hash,
                issued_at: parse_date(&issued_at)?,
                expires_at: parse_date(&expires_at)?,
                last_heartbeat_at: parse_date(&last_heartbeat_at)?,
            })
        },
    )
    .transpose()
}

fn query_had_expired_lease(
    connection: &Connection,
    work_item_id: &WorkItemId,
) -> Result<bool, StorageError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM leases WHERE work_item_id = ?1 AND status = 'expired')",
        [work_item_id.to_string()],
        |row| row.get(0),
    )?)
}

#[derive(Debug)]
struct StoredRunRow {
    id: String,
    work_item_id: String,
    lease_id: String,
    agent_profile: String,
    claim_request_id: String,
    status: String,
    verification_snapshot_json: Option<String>,
    verification_snapshot_hash: Option<String>,
    review_snapshot_json: Option<String>,
    review_snapshot_hash: Option<String>,
    started_at: String,
    ended_at: Option<String>,
}

impl StoredRunRow {
    fn into_run(self) -> Result<WorkRun, StorageError> {
        Ok(WorkRun {
            id: parse_id(&self.id, "work run")?,
            work_item_id: parse_id(&self.work_item_id, "work item")?,
            lease_id: parse_id(&self.lease_id, "lease")?,
            agent_profile: self.agent_profile,
            claim_request_id: self.claim_request_id,
            status: parse_enum(&self.status, "work run status")?,
            verification_snapshot: self
                .verification_snapshot_json
                .map(|value| serde_json::from_str(&value))
                .transpose()?,
            verification_snapshot_hash: self.verification_snapshot_hash,
            review_snapshot: self
                .review_snapshot_json
                .map(|value| serde_json::from_str(&value))
                .transpose()?,
            review_snapshot_hash: self.review_snapshot_hash,
            started_at: parse_date(&self.started_at)?,
            ended_at: parse_optional_date(self.ended_at)?,
        })
    }
}

fn read_run_row(row: &Row<'_>) -> rusqlite::Result<StoredRunRow> {
    Ok(StoredRunRow {
        id: row.get(0)?,
        work_item_id: row.get(1)?,
        lease_id: row.get(2)?,
        agent_profile: row.get(3)?,
        claim_request_id: row.get(4)?,
        status: row.get(5)?,
        verification_snapshot_json: row.get(6)?,
        verification_snapshot_hash: row.get(7)?,
        review_snapshot_json: row.get(8)?,
        review_snapshot_hash: row.get(9)?,
        started_at: row.get(10)?,
        ended_at: row.get(11)?,
    })
}

fn query_active_run_for_lease(
    connection: &Connection,
    lease_id: &LeaseId,
) -> Result<Option<WorkRun>, StorageError> {
    let row = connection
        .query_row(
            "SELECT id, work_item_id, lease_id, agent_profile, claim_request_id,
                    status, verification_snapshot_json, verification_snapshot_hash,
                    review_snapshot_json, review_snapshot_hash, started_at, ended_at
             FROM work_runs WHERE lease_id = ?1 AND ended_at IS NULL
             ORDER BY started_at DESC LIMIT 1",
            [lease_id.to_string()],
            read_run_row,
        )
        .optional()?;
    row.map(StoredRunRow::into_run).transpose()
}

#[derive(Debug)]
struct StoredReportRow {
    id: String,
    work_item_id: String,
    work_run_id: String,
    contract_hash: String,
    report_json: String,
    report_hash: String,
    accepted: i64,
    rejection_reasons_json: String,
    created_at: String,
    verification_status: VerificationStatus,
    verification: Option<VerificationResult>,
    review_status: ReviewStatus,
    review: Option<coldstart_core::ReviewResult>,
    review_waiver: Option<coldstart_core::ReviewWaiver>,
}

impl StoredReportRow {
    fn into_record(self) -> Result<WorkReportRecord, StorageError> {
        Ok(WorkReportRecord {
            id: parse_id(&self.id, "work report")?,
            work_item_id: parse_id(&self.work_item_id, "work item")?,
            work_run_id: parse_id(&self.work_run_id, "work run")?,
            contract_hash: self.contract_hash,
            report: serde_json::from_str(&self.report_json)?,
            report_hash: self.report_hash,
            accepted: self.accepted != 0,
            rejection_reasons: serde_json::from_str(&self.rejection_reasons_json)?,
            verification_status: self.verification_status,
            verification: self.verification,
            review_status: self.review_status,
            review: self.review,
            review_waiver: self.review_waiver,
            created_at: parse_date(&self.created_at)?,
        })
    }
}

fn read_report_row(row: &Row<'_>) -> rusqlite::Result<StoredReportRow> {
    let verification = read_verification_fields(row, 9, 10, 11, 12)?;
    let review_status: String = row.get(13)?;
    let review_json: Option<String> = row.get(14)?;
    let _review_started_at: Option<String> = row.get(15)?;
    let _review_finished_at: Option<String> = row.get(16)?;
    let review_waiver_json: Option<String> = row.get(17)?;
    let review_status = ReviewStatus::parse(&review_status).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unknown review status: {review_status}"),
            )),
        )
    })?;
    let review = review_json
        .map(|value| serde_json::from_str(&value))
        .transpose()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    let review_waiver = review_waiver_json
        .map(|value| serde_json::from_str(&value))
        .transpose()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    Ok(StoredReportRow {
        id: row.get(0)?,
        work_item_id: row.get(1)?,
        work_run_id: row.get(2)?,
        contract_hash: row.get(3)?,
        report_json: row.get(4)?,
        report_hash: row.get(5)?,
        accepted: row.get(6)?,
        rejection_reasons_json: row.get(7)?,
        created_at: row.get(8)?,
        verification_status: verification.status,
        verification: verification.result,
        review_status,
        review,
        review_waiver,
    })
}

pub(crate) fn query_report_by_idempotency(
    connection: &Connection,
    work_item_id: &WorkItemId,
    idempotency_hash: &str,
) -> Result<Option<WorkReportRecord>, StorageError> {
    let row = connection
        .query_row(
            "SELECT id, work_item_id, work_run_id, contract_hash, report_json,
                    report_hash, accepted, rejection_reasons_json, created_at,
                    verification_status, verification_json, verification_started_at,
                    verification_finished_at, review_status, review_json,
                    review_started_at, review_finished_at, review_waiver_json
             FROM work_reports WHERE work_item_id = ?1 AND idempotency_key_hash = ?2",
            params![work_item_id.to_string(), idempotency_hash],
            read_report_row,
        )
        .optional()?;
    row.map(StoredReportRow::into_record).transpose()
}

pub(crate) fn query_report_by_id(
    connection: &Connection,
    report_id: &WorkReportId,
) -> Result<Option<WorkReportRecord>, StorageError> {
    let row = connection
        .query_row(
            "SELECT id, work_item_id, work_run_id, contract_hash, report_json,
                    report_hash, accepted, rejection_reasons_json, created_at,
                    verification_status, verification_json, verification_started_at,
                    verification_finished_at, review_status, review_json,
                    review_started_at, review_finished_at, review_waiver_json
             FROM work_reports WHERE id = ?1",
            [report_id.to_string()],
            read_report_row,
        )
        .optional()?;
    row.map(StoredReportRow::into_record).transpose()
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}

pub(crate) fn refresh_work_items(
    transaction: &Transaction<'_>,
    project_id: &ProjectId,
    plan_revision_id: &PlanRevisionId,
    now: DateTime<Utc>,
) -> Result<(), StorageError> {
    loop {
        let mut statement = transaction.prepare(
            "SELECT w.id, w.task_key, w.status, t.spec_json
             FROM work_items w JOIN task_specs t ON t.id = w.task_spec_id
             WHERE w.plan_revision_id = ?1",
        )?;
        let rows = statement.query_map([plan_revision_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let mut items = Vec::new();
        for row in rows {
            let (id, task_key, status, spec_json) = row?;
            items.push((
                id,
                task_key,
                parse_enum::<WorkItemStatus>(&status, "work item status")?,
                serde_json::from_str::<TaskSpec>(&spec_json)?,
            ));
        }
        drop(statement);
        let statuses = items
            .iter()
            .map(|(_, key, status, _)| (key.clone(), *status))
            .collect::<BTreeMap<_, _>>();
        let mut milestone_completed = false;
        for (id, task_key, status, task) in &items {
            if task.kind == TaskKind::Milestone
                && matches!(
                    status,
                    WorkItemStatus::Draft | WorkItemStatus::Ready | WorkItemStatus::Reopened
                )
                && dependencies_done(task, &statuses)
            {
                transaction.execute(
                    "UPDATE work_items SET status = 'done', updated_at = ?1, completed_at = ?1
                     WHERE id = ?2",
                    params![now.to_rfc3339(), id],
                )?;
                insert_event(
                    transaction,
                    &DomainEvent::new(
                        *project_id,
                        DomainEventKind::WorkCompleted {
                            work_item_id: parse_id(id, "work item")?,
                            task_key: task_key.clone(),
                        },
                    ),
                )?;
                milestone_completed = true;
            }
        }
        if milestone_completed {
            continue;
        }

        for (id, _, status, task) in &items {
            let next = if dependencies_done(task, &statuses) {
                match status {
                    WorkItemStatus::Draft => Some(WorkItemStatus::Ready),
                    WorkItemStatus::Ready => None,
                    WorkItemStatus::Reopened => Some(WorkItemStatus::Ready),
                    _ => None,
                }
            } else if *status == WorkItemStatus::Ready {
                Some(WorkItemStatus::Draft)
            } else {
                None
            };
            if let Some(next) = next {
                transaction.execute(
                    "UPDATE work_items SET status = ?1, updated_at = ?2 WHERE id = ?3",
                    params![next.as_str(), now.to_rfc3339(), id],
                )?;
            }
        }
        break;
    }
    Ok(())
}

fn dependencies_done(task: &TaskSpec, statuses: &BTreeMap<String, WorkItemStatus>) -> bool {
    task.blocking_dependencies().all(|dependency| {
        statuses
            .get(dependency)
            .is_some_and(|status| *status == WorkItemStatus::Done)
    })
}

#[derive(Debug)]
struct StoredPlanRow {
    id: String,
    project_id: String,
    revision_number: i64,
    blueprint_revision_id: String,
    blueprint_content_hash: String,
    template_version: String,
    status: String,
    content_json: String,
    content_hash: String,
    created_at: String,
    published_at: Option<String>,
}

impl StoredPlanRow {
    fn into_plan(self) -> Result<PlanRevision, StorageError> {
        Ok(PlanRevision {
            id: parse_id(&self.id, "plan revision")?,
            project_id: parse_id(&self.project_id, "project")?,
            revision_number: self.revision_number as u64,
            blueprint_revision_id: parse_id(&self.blueprint_revision_id, "blueprint revision")?,
            blueprint_content_hash: self.blueprint_content_hash,
            template_version: self.template_version,
            status: parse_enum(&self.status, "plan status")?,
            content: serde_json::from_str(&self.content_json)?,
            content_hash: self.content_hash,
            created_at: parse_date(&self.created_at)?,
            published_at: parse_optional_date(self.published_at)?,
        })
    }
}

fn read_plan_row(row: &Row<'_>) -> rusqlite::Result<StoredPlanRow> {
    Ok(StoredPlanRow {
        id: row.get(0)?,
        project_id: row.get(1)?,
        revision_number: row.get(2)?,
        blueprint_revision_id: row.get(3)?,
        blueprint_content_hash: row.get(4)?,
        template_version: row.get(5)?,
        status: row.get(6)?,
        content_json: row.get(7)?,
        content_hash: row.get(8)?,
        created_at: row.get(9)?,
        published_at: row.get(10)?,
    })
}

#[derive(Debug)]
struct StoredWorkItemRow {
    id: String,
    project_id: String,
    plan_revision_id: String,
    task_spec_id: String,
    task_key: String,
    status: String,
    contract_state: String,
    priority: i64,
    created_at: String,
    updated_at: String,
    completed_at: Option<String>,
}

impl StoredWorkItemRow {
    fn into_work_item(self) -> Result<WorkItem, StorageError> {
        Ok(WorkItem {
            id: parse_id(&self.id, "work item")?,
            project_id: parse_id(&self.project_id, "project")?,
            plan_revision_id: parse_id(&self.plan_revision_id, "plan revision")?,
            task_spec_id: parse_id(&self.task_spec_id, "task spec")?,
            task_key: self.task_key,
            status: parse_enum(&self.status, "work item status")?,
            contract_state: parse_enum(&self.contract_state, "contract state")?,
            priority: self.priority,
            created_at: parse_date(&self.created_at)?,
            updated_at: parse_date(&self.updated_at)?,
            completed_at: parse_optional_date(self.completed_at)?,
        })
    }
}

fn read_work_item_row(row: &Row<'_>) -> rusqlite::Result<StoredWorkItemRow> {
    Ok(StoredWorkItemRow {
        id: row.get(0)?,
        project_id: row.get(1)?,
        plan_revision_id: row.get(2)?,
        task_spec_id: row.get(3)?,
        task_key: row.get(4)?,
        status: row.get(5)?,
        contract_state: row.get(6)?,
        priority: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        completed_at: row.get(10)?,
    })
}

fn query_plan(
    connection: &Connection,
    id: &PlanRevisionId,
) -> Result<Option<PlanRevision>, StorageError> {
    let row = connection
        .query_row(
            "SELECT id, project_id, revision_number, blueprint_revision_id,
                    blueprint_content_hash, template_version, status,
                    content_json, content_hash, created_at, published_at
             FROM plan_revisions WHERE id = ?1",
            [id.to_string()],
            read_plan_row,
        )
        .optional()?;
    row.map(StoredPlanRow::into_plan).transpose()
}

fn parse_enum<T>(value: &str, field: &'static str) -> Result<T, StorageError>
where
    T: FromStr,
    T::Err: Display,
{
    value.parse().map_err(|_| StorageError::InvalidEnumValue {
        field,
        value: value.to_owned(),
    })
}

fn parse_id<T>(value: &str, kind: &'static str) -> Result<T, StorageError>
where
    T: FromStr,
    T::Err: Display,
{
    value.parse().map_err(|_| StorageError::InvalidId {
        kind,
        value: value.to_owned(),
    })
}

fn parse_date(value: &str) -> Result<DateTime<Utc>, StorageError> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

fn parse_optional_date(value: Option<String>) -> Result<Option<DateTime<Utc>>, StorageError> {
    value.map(|value| parse_date(&value)).transpose()
}
