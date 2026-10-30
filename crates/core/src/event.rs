use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    ArtifactId, BlueprintRevisionId, GenerationSource, InfoRequestId, Project, ProjectFactId,
    ProjectId,
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
        }
    }
}
