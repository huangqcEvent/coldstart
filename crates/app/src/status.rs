use std::path::PathBuf;

use chrono::{DateTime, Utc};
use coldstart_core::ProjectId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStatus {
    pub project_id: ProjectId,
    pub name: String,
    pub root_path: PathBuf,
    pub created_at: DateTime<Utc>,
    pub event_count: u64,
}
