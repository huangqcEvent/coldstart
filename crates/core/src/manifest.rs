use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Project, ProjectId};

pub const PROJECT_MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectManifest {
    pub schema_version: u32,
    pub project_id: ProjectId,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

impl ProjectManifest {
    pub fn from_project(project: &Project) -> Self {
        Self {
            schema_version: PROJECT_MANIFEST_SCHEMA_VERSION,
            project_id: project.id,
            name: project.name.clone(),
            created_at: project.created_at,
        }
    }

    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.schema_version != PROJECT_MANIFEST_SCHEMA_VERSION {
            return Err(ManifestError::UnsupportedSchema {
                found: self.schema_version,
                supported: PROJECT_MANIFEST_SCHEMA_VERSION,
            });
        }

        let name = self.name.trim();
        if name.is_empty() {
            return Err(ManifestError::EmptyName);
        }
        if name.chars().count() > 80 {
            return Err(ManifestError::NameTooLong);
        }

        Ok(())
    }

    pub fn to_project(&self, root_path: PathBuf) -> Result<Project, ManifestError> {
        self.validate()?;
        Ok(Project {
            id: self.project_id,
            name: self.name.trim().to_owned(),
            root_path,
            created_at: self.created_at,
        })
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ManifestError {
    #[error("unsupported project manifest schema {found}; supported schema is {supported}")]
    UnsupportedSchema { found: u32, supported: u32 },
    #[error("project name cannot be empty")]
    EmptyName,
    #[error("project name must be at most 80 characters")]
    NameTooLong,
}
