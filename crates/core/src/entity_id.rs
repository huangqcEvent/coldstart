//! 强类型实体 ID。
//!
//! 不同领域实体使用不同 ID 类型，防止跨实体误用；构造和派生时统一校验字符集、
//! 大小写和空白，确保数据库、事件与 MCP 协议使用同一标识格式。

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::ProjectId;

macro_rules! uuid_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Ok(Self(Uuid::parse_str(value)?))
            }
        }
    };
}

uuid_id!(ArtifactId);
uuid_id!(BlueprintRevisionId);
uuid_id!(CapabilityApiEndpointId);
uuid_id!(CodeInvestigationId);
uuid_id!(CredentialId);
uuid_id!(GitSyncId);
uuid_id!(InfoRequestId);
uuid_id!(LeaseId);
uuid_id!(PlanRevisionId);
uuid_id!(ProjectFactId);
uuid_id!(PullRequestId);
uuid_id!(RepositoryId);
uuid_id!(ReviewStandardId);
uuid_id!(ReviewStandardRevisionId);
uuid_id!(TaskSpecId);
uuid_id!(WorkItemId);
uuid_id!(WorkReportId);
uuid_id!(WorkRunId);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BlueprintEntityId(String);

impl BlueprintEntityId {
    pub const MAX_LENGTH: usize = 200;

    pub fn derive(project_id: &ProjectId, kind: &str, name: &str) -> Self {
        let normalized = normalize_entity_name(name);
        let key = format!("coldstart:{project_id}:{kind}:{normalized}");
        let id = Uuid::new_v5(&Uuid::NAMESPACE_URL, key.as_bytes());
        Self(format!("{kind}_{}", id.simple()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for BlueprintEntityId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for BlueprintEntityId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for BlueprintEntityId {
    type Err = EntityIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        if value.is_empty() {
            return Err(EntityIdError::Empty);
        }
        if value.chars().count() > Self::MAX_LENGTH {
            return Err(EntityIdError::TooLong);
        }
        if !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        {
            return Err(EntityIdError::InvalidCharacters);
        }
        Ok(Self(value.to_owned()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EntityIdError {
    #[error("entity id cannot be empty")]
    Empty,
    #[error("entity id must be at most {max} characters", max = BlueprintEntityId::MAX_LENGTH)]
    TooLong,
    #[error("entity id may only contain ASCII letters, digits, underscores, and hyphens")]
    InvalidCharacters,
}

pub(crate) fn normalize_entity_name(value: &str) -> String {
    let mut normalized = String::new();
    let mut pending_space = false;
    for character in value.trim().chars() {
        if character.is_whitespace() {
            pending_space = !normalized.is_empty();
            continue;
        }
        if pending_space {
            normalized.push(' ');
            pending_space = false;
        }
        for lower in character.to_lowercase() {
            normalized.push(lower);
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProjectId;

    #[test]
    fn derived_ids_ignore_case_and_whitespace_differences() {
        let project_id = ProjectId::new();

        assert_eq!(
            BlueprintEntityId::derive(&project_id, "module", "Order   Service"),
            BlueprintEntityId::derive(&project_id, "module", " order service ")
        );
    }

    #[test]
    fn entity_ids_reject_path_characters() {
        assert_eq!(
            "../outside".parse::<BlueprintEntityId>(),
            Err(EntityIdError::InvalidCharacters)
        );
    }
}
