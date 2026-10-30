use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::ProjectId;

macro_rules! uuid_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
uuid_id!(InfoRequestId);
uuid_id!(ProjectFactId);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
