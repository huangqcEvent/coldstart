//! 待确认信息与阻塞问题模型。
//!
//! 信息请求代表必须由人工补充的事实。指纹用于去重，回答后转化为项目事实，
//! 不应通过修改旧请求来覆盖历史。

use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{BlueprintRevisionId, InfoRequestId, ProjectFactId, ProjectId, ProposedInfoRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InfoRequestCategory {
    Information,
    Decision,
    Authorization,
    Approval,
    ExternalOperation,
}

impl InfoRequestCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Information => "information",
            Self::Decision => "decision",
            Self::Authorization => "authorization",
            Self::Approval => "approval",
            Self::ExternalOperation => "external_operation",
        }
    }
}

impl fmt::Display for InfoRequestCategory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for InfoRequestCategory {
    type Err = InfoRequestEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "information" => Ok(Self::Information),
            "decision" => Ok(Self::Decision),
            "authorization" => Ok(Self::Authorization),
            "approval" => Ok(Self::Approval),
            "external_operation" => Ok(Self::ExternalOperation),
            _ => Err(InfoRequestEnumError {
                kind: "category",
                value: value.to_owned(),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfoRequestStatus {
    Open,
    Answered,
    Dismissed,
}

impl InfoRequestStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Answered => "answered",
            Self::Dismissed => "dismissed",
        }
    }
}

impl fmt::Display for InfoRequestStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for InfoRequestStatus {
    type Err = InfoRequestEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "open" => Ok(Self::Open),
            "answered" => Ok(Self::Answered),
            "dismissed" => Ok(Self::Dismissed),
            _ => Err(InfoRequestEnumError {
                kind: "status",
                value: value.to_owned(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unknown info request {kind}: {value}")]
pub struct InfoRequestEnumError {
    kind: &'static str,
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoRequestDraft {
    pub category: InfoRequestCategory,
    pub question: String,
    pub reason: String,
    pub example: Option<String>,
    pub default_value: Option<String>,
    pub impact_scope: String,
    pub due_at: Option<DateTime<Utc>>,
    pub blocking: bool,
}

impl InfoRequestDraft {
    pub fn from_proposal(
        proposal: ProposedInfoRequest,
    ) -> Result<Self, InfoRequestConversionError> {
        let question = proposal.question.trim().to_owned();
        if question.is_empty() {
            return Err(InfoRequestConversionError::EmptyQuestion);
        }
        let due_at = proposal
            .due_at
            .filter(|value| !value.trim().is_empty())
            .map(|value| {
                DateTime::parse_from_rfc3339(&value)
                    .map(|date| date.with_timezone(&Utc))
                    .map_err(|_| InfoRequestConversionError::InvalidDueAt(value))
            })
            .transpose()?;

        Ok(Self {
            category: proposal.category,
            question,
            reason: proposal.reason.trim().to_owned(),
            example: trim_optional(proposal.example),
            default_value: trim_optional(proposal.default_value),
            impact_scope: proposal.impact_scope.trim().to_owned(),
            due_at,
            blocking: proposal.blocking,
        })
    }

    pub fn fingerprint(&self) -> String {
        info_request_fingerprint(self.category, &self.question)
    }
}

fn trim_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

pub fn info_request_fingerprint(category: InfoRequestCategory, question: &str) -> String {
    let normalized = question
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let mut digest = Sha256::new();
    digest.update(category.as_str().as_bytes());
    digest.update(b":");
    digest.update(normalized.as_bytes());
    format!("{:x}", digest.finalize())
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InfoRequestConversionError {
    #[error("info request question cannot be empty")]
    EmptyQuestion,
    #[error("invalid RFC 3339 due date in model output: {0}")]
    InvalidDueAt(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoRequest {
    pub id: InfoRequestId,
    pub project_id: ProjectId,
    pub blueprint_revision_id: BlueprintRevisionId,
    pub fingerprint: String,
    pub category: InfoRequestCategory,
    pub question: String,
    pub reason: String,
    pub example: Option<String>,
    pub default_value: Option<String>,
    pub impact_scope: String,
    pub due_at: Option<DateTime<Utc>>,
    pub blocking: bool,
    pub status: InfoRequestStatus,
    pub answer: Option<String>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFact {
    pub id: ProjectFactId,
    pub project_id: ProjectId,
    pub fact_key: String,
    pub category: InfoRequestCategory,
    pub question: String,
    pub value: String,
    pub version: u64,
    pub source_info_request_id: InfoRequestId,
    pub created_at: DateTime<Utc>,
}
