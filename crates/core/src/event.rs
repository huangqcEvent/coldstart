use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    ArtifactId, BlueprintRevisionId, CredentialId, GenerationSource, GitSyncId, InfoRequestId,
    LeaseId, PlanRevisionId, Project, ProjectFactId, ProjectId, RepositoryId, WorkItemId,
    WorkReportId,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainEvent {
    pub event_id: Uuid,
    pub project_id: ProjectId,
    pub occurred_at: DateTime<Utc>,
    pub kind: DomainEventKind,
}

impl DomainEvent {
    pub fn new(project_id: ProjectId, kind: DomainEventKind) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            project_id,
            occurred_at: Utc::now(),
            kind,
        }
    }

    pub fn project_initialized(project: &Project) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            project_id: project.id,
            occurred_at: Utc::now(),
            kind: DomainEventKind::ProjectInitialized {
                name: project.name.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum DomainEventKind {
    ProjectInitialized {
        name: String,
    },
    ArtifactImported {
        artifact_id: ArtifactId,
        file_name: String,
        content_hash: String,
    },
    BlueprintDraftCreated {
        revision_id: BlueprintRevisionId,
        revision_number: u64,
        generation_source: GenerationSource,
    },
    BlueprintPublished {
        revision_id: BlueprintRevisionId,
        revision_number: u64,
    },
    InfoRequestCreated {
        request_id: InfoRequestId,
        question: String,
    },
    InfoRequestResolved {
        request_id: InfoRequestId,
        fact_id: ProjectFactId,
        version: u64,
    },
    PlanDraftCreated {
        revision_id: PlanRevisionId,
        revision_number: u64,
        task_count: usize,
    },
    PlanPublished {
        revision_id: PlanRevisionId,
        revision_number: u64,
        task_count: usize,
    },
    PlanSuperseded {
        revision_id: PlanRevisionId,
        superseded_by: PlanRevisionId,
    },
    WorkClaimed {
        work_item_id: WorkItemId,
        task_key: String,
        lease_id: LeaseId,
        agent_profile: String,
    },
    WorkStarted {
        work_item_id: WorkItemId,
    },
    WorkHeartbeat {
        work_item_id: WorkItemId,
        lease_id: LeaseId,
        expires_at: DateTime<Utc>,
    },
    WorkBlocked {
        work_item_id: WorkItemId,
        lease_id: LeaseId,
        reason: String,
    },
    WorkReportSubmitted {
        work_item_id: WorkItemId,
        report_id: WorkReportId,
        accepted: bool,
    },
    WorkCompleted {
        work_item_id: WorkItemId,
        task_key: String,
    },
    WorkReleased {
        work_item_id: WorkItemId,
        lease_id: LeaseId,
        reason: String,
    },
    LeaseExpired {
        work_item_id: WorkItemId,
        lease_id: LeaseId,
    },
    HumanWorkCompleted {
        work_item_id: WorkItemId,
        accepted: bool,
    },
    RepositorySaved {
        repository_id: RepositoryId,
        repository_key: String,
        remote_url: String,
    },
    CredentialSaved {
        credential_id: CredentialId,
        provider: String,
        environment: String,
    },
    CredentialDeleted {
        credential_id: CredentialId,
    },
    CapabilityUpdated {
        capability_id: String,
        environment: String,
        status: String,
    },
    GitSyncRecorded {
        sync_id: GitSyncId,
        work_item_id: WorkItemId,
        repository_id: RepositoryId,
        status: String,
    },
    ProjectSettingUpdated {
        setting_key: String,
    },
}

impl DomainEventKind {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::ProjectInitialized { .. } => "project_initialized",
            Self::ArtifactImported { .. } => "artifact_imported",
            Self::BlueprintDraftCreated { .. } => "blueprint_draft_created",
            Self::BlueprintPublished { .. } => "blueprint_published",
            Self::InfoRequestCreated { .. } => "info_request_created",
            Self::InfoRequestResolved { .. } => "info_request_resolved",
            Self::PlanDraftCreated { .. } => "plan_draft_created",
            Self::PlanPublished { .. } => "plan_published",
            Self::PlanSuperseded { .. } => "plan_superseded",
            Self::WorkClaimed { .. } => "work_claimed",
            Self::WorkStarted { .. } => "work_started",
            Self::WorkHeartbeat { .. } => "work_heartbeat",
            Self::WorkBlocked { .. } => "work_blocked",
            Self::WorkReportSubmitted { .. } => "work_report_submitted",
            Self::WorkCompleted { .. } => "work_completed",
            Self::WorkReleased { .. } => "work_released",
            Self::LeaseExpired { .. } => "lease_expired",
            Self::HumanWorkCompleted { .. } => "human_work_completed",
            Self::RepositorySaved { .. } => "repository_saved",
            Self::CredentialSaved { .. } => "credential_saved",
            Self::CredentialDeleted { .. } => "credential_deleted",
            Self::CapabilityUpdated { .. } => "capability_updated",
            Self::GitSyncRecorded { .. } => "git_sync_recorded",
            Self::ProjectSettingUpdated { .. } => "project_setting_updated",
        }
    }
}
