use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Project, ProjectId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainEvent {
    pub event_id: Uuid,
    pub project_id: ProjectId,
    pub occurred_at: DateTime<Utc>,
    pub kind: DomainEventKind,
}

impl DomainEvent {
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
    ProjectInitialized { name: String },
}

impl DomainEventKind {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::ProjectInitialized { .. } => "project_initialized",
        }
    }
}
