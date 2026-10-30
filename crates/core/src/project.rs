use std::{fmt, path::PathBuf, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ProjectError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProjectId(Uuid);

impl ProjectId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ProjectId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ProjectId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for ProjectId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(value)?))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub root_path: PathBuf,
    pub created_at: DateTime<Utc>,
}

impl Project {
    pub fn new(name: impl Into<String>, root_path: PathBuf) -> Result<Self, ProjectError> {
        let name = name.into().trim().to_owned();

        if name.is_empty() {
            return Err(ProjectError::EmptyName);
        }

        if name.chars().count() > 80 {
            return Err(ProjectError::NameTooLong);
        }

        Ok(Self {
            id: ProjectId::new(),
            name,
            root_path,
            created_at: Utc::now(),
        })
    }
}
