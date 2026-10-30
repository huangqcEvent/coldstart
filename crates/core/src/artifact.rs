use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ArtifactId, ProjectId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactFormat {
    Markdown,
    Text,
    Docx,
    Pdf,
}

impl ArtifactFormat {
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension
            .trim_start_matches('.')
            .to_ascii_lowercase()
            .as_str()
        {
            "md" | "markdown" => Some(Self::Markdown),
            "txt" => Some(Self::Text),
            "docx" => Some(Self::Docx),
            "pdf" => Some(Self::Pdf),
            _ => None,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Text => "txt",
            Self::Docx => "docx",
            Self::Pdf => "pdf",
        }
    }
}

impl fmt::Display for ArtifactFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Markdown => "markdown",
            Self::Text => "text",
            Self::Docx => "docx",
            Self::Pdf => "pdf",
        })
    }
}

impl FromStr for ArtifactFormat {
    type Err = ArtifactFormatError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "markdown" => Ok(Self::Markdown),
            "text" => Ok(Self::Text),
            "docx" => Ok(Self::Docx),
            "pdf" => Ok(Self::Pdf),
            _ => Err(ArtifactFormatError(value.to_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unknown artifact format: {0}")]
pub struct ArtifactFormatError(String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentAnchor {
    pub id: String,
    pub label: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactMetadata {
    pub id: ArtifactId,
    pub project_id: ProjectId,
    pub file_name: String,
    pub format: ArtifactFormat,
    pub content_hash: String,
    pub text_hash: String,
    pub byte_size: u64,
    pub import_source: String,
    pub imported_at: DateTime<Utc>,
}
