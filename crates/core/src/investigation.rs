//! 历史项目只读代码调查模型。
//!
//! 调查请求与开发任务隔离：Codex 只能读取已映射仓库并提交结构化证据，调查结果
//! 在人工确认前不能进入蓝图模型上下文，更不能直接创建代码修改任务。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    CodeInvestigationId, ProjectId, RepositoryId, is_safe_relative_path, validate_branch_name,
};

pub const MAX_CODE_FINDINGS: usize = 40;
pub const MAX_CODE_FINDING_CHARS: usize = 2_000;
pub const MAX_CODE_FINDING_PATH_CHARS: usize = 500;
pub const MAX_CODE_SUGGESTED_PATHS: usize = 20;
pub const MAX_CODE_QUESTION_CHARS: usize = 4_000;
pub const CODE_INVESTIGATION_LEASE_MINUTES: i64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeInvestigationStatus {
    Requested,
    Claimed,
    Submitted,
    Confirmed,
    Rejected,
}

impl CodeInvestigationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Claimed => "claimed",
            Self::Submitted => "submitted",
            Self::Confirmed => "confirmed",
            Self::Rejected => "rejected",
        }
    }
}

/// 一条可定位到仓库路径与行号的只读代码证据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeFinding {
    pub path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub symbol: Option<String>,
    pub summary: String,
    pub risk: String,
    #[serde(default)]
    pub suggested_change_paths: Vec<String>,
}

/// Codex 针对一次调查请求提交的不可变报告，不包含代码正文或 Secret。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeInvestigationReport {
    pub commit_sha: String,
    pub summary: String,
    pub findings: Vec<CodeFinding>,
    #[serde(default)]
    pub limitations: Vec<String>,
    pub idempotency_key: String,
}

impl CodeInvestigationReport {
    pub fn validate(&self) -> Result<(), CodeInvestigationValidationError> {
        validate_commit_sha(&self.commit_sha)?;
        validate_required_text("summary", &self.summary, MAX_CODE_FINDING_CHARS)?;
        validate_required_text("idempotency key", &self.idempotency_key, 200)?;
        if self.findings.is_empty() || self.findings.len() > MAX_CODE_FINDINGS {
            return Err(CodeInvestigationValidationError::FindingCount {
                actual: self.findings.len(),
            });
        }
        for finding in &self.findings {
            validate_finding(finding)?;
        }
        if self.limitations.len() > 20 {
            return Err(CodeInvestigationValidationError::TooManyLimitations);
        }
        for limitation in &self.limitations {
            validate_required_text("limitation", limitation, MAX_CODE_FINDING_CHARS)?;
        }
        Ok(())
    }

    pub fn content_hash(&self) -> String {
        let encoded = serde_json::to_vec(self).expect("investigation report must serialize");
        let mut digest = Sha256::new();
        digest.update(encoded);
        format!("{:x}", digest.finalize())
    }
}

/// 持久化后的调查记录。领取令牌只通过一次性 MCP 返回值传递，不在此结构中保存。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeInvestigation {
    pub id: CodeInvestigationId,
    pub project_id: ProjectId,
    pub repository_id: RepositoryId,
    pub repository_key: String,
    pub base_ref: String,
    pub question: String,
    pub status: CodeInvestigationStatus,
    pub agent_profile: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub commit_sha: Option<String>,
    pub summary: Option<String>,
    pub findings: Vec<CodeFinding>,
    pub limitations: Vec<String>,
    pub report_hash: Option<String>,
    pub rejection_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub confirmed_at: Option<DateTime<Utc>>,
}

/// 主动领取调查时返回给 Agent 的一次性凭据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeInvestigationClaim {
    pub investigation: CodeInvestigation,
    pub claim_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CodeInvestigationValidationError {
    #[error("investigation question must be {min} to {max} characters")]
    QuestionLength { min: usize, max: usize },
    #[error("commit SHA must be a full 40-character hexadecimal object id")]
    InvalidCommitSha,
    #[error("{field} must be non-empty and at most {max} characters")]
    InvalidText { field: String, max: usize },
    #[error("an investigation must contain 1 to {MAX_CODE_FINDINGS} findings")]
    FindingCount { actual: usize },
    #[error(
        "finding path must be a safe repository-relative path of at most {MAX_CODE_FINDING_PATH_CHARS} characters"
    )]
    InvalidPath,
    #[error("finding line range is invalid: {start}..{end}")]
    InvalidLineRange { start: u32, end: u32 },
    #[error("finding contains too many suggested change paths")]
    TooManySuggestedPaths,
    #[error("investigation contains too many limitations")]
    TooManyLimitations,
    #[error("invalid branch name: {0}")]
    InvalidBranch(String),
}

pub fn validate_investigation_question(
    question: &str,
) -> Result<(), CodeInvestigationValidationError> {
    let count = question.trim().chars().count();
    if !(5..=MAX_CODE_QUESTION_CHARS).contains(&count) {
        return Err(CodeInvestigationValidationError::QuestionLength {
            min: 5,
            max: MAX_CODE_QUESTION_CHARS,
        });
    }
    Ok(())
}

pub fn validate_investigation_base_ref(
    base_ref: &str,
) -> Result<(), CodeInvestigationValidationError> {
    validate_branch_name(base_ref)
        .map_err(|_| CodeInvestigationValidationError::InvalidBranch(base_ref.to_owned()))
}

fn validate_finding(finding: &CodeFinding) -> Result<(), CodeInvestigationValidationError> {
    if finding.path.chars().count() > MAX_CODE_FINDING_PATH_CHARS
        || !is_safe_relative_path(&finding.path)
    {
        return Err(CodeInvestigationValidationError::InvalidPath);
    }
    if finding.line_start == 0 || finding.line_end < finding.line_start {
        return Err(CodeInvestigationValidationError::InvalidLineRange {
            start: finding.line_start,
            end: finding.line_end,
        });
    }
    if let Some(symbol) = &finding.symbol {
        validate_required_text("symbol", symbol, 300)?;
    }
    validate_required_text("finding summary", &finding.summary, MAX_CODE_FINDING_CHARS)?;
    validate_required_text("finding risk", &finding.risk, MAX_CODE_FINDING_CHARS)?;
    if finding.suggested_change_paths.len() > MAX_CODE_SUGGESTED_PATHS {
        return Err(CodeInvestigationValidationError::TooManySuggestedPaths);
    }
    for path in &finding.suggested_change_paths {
        if path.chars().count() > MAX_CODE_FINDING_PATH_CHARS || !is_safe_relative_path(path) {
            return Err(CodeInvestigationValidationError::InvalidPath);
        }
    }
    Ok(())
}

fn validate_commit_sha(value: &str) -> Result<(), CodeInvestigationValidationError> {
    let value = value.trim();
    if value.len() != 40 || !value.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(CodeInvestigationValidationError::InvalidCommitSha);
    }
    Ok(())
}

fn validate_required_text(
    field: &str,
    value: &str,
    max: usize,
) -> Result<(), CodeInvestigationValidationError> {
    let count = value.trim().chars().count();
    if count == 0 || count > max {
        return Err(CodeInvestigationValidationError::InvalidText {
            field: field.to_owned(),
            max,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding() -> CodeFinding {
        CodeFinding {
            path: "src/orders/service.rs".to_owned(),
            line_start: 20,
            line_end: 48,
            symbol: Some("create_order".to_owned()),
            summary: "订单创建会复用既有库存扣减服务".to_owned(),
            risk: "新增优惠逻辑可能绕过库存一致性检查".to_owned(),
            suggested_change_paths: vec!["src/orders/service.rs".to_owned()],
        }
    }

    #[test]
    fn validates_a_structured_read_only_report() {
        let report = CodeInvestigationReport {
            commit_sha: "a".repeat(40),
            summary: "历史订单模块已有库存服务，可复用".to_owned(),
            findings: vec![finding()],
            limitations: vec!["未检查生产配置".to_owned()],
            idempotency_key: "investigation-1".to_owned(),
        };
        assert!(report.validate().is_ok());
        assert_eq!(report.content_hash().len(), 64);
    }

    #[test]
    fn rejects_unsafe_paths_and_short_commit_ids() {
        let mut unsafe_finding = finding();
        unsafe_finding.path = "../secrets".to_owned();
        let report = CodeInvestigationReport {
            commit_sha: "abc123".to_owned(),
            summary: "summary".to_owned(),
            findings: vec![unsafe_finding],
            limitations: vec![],
            idempotency_key: "investigation-2".to_owned(),
        };
        assert_eq!(
            report.validate(),
            Err(CodeInvestigationValidationError::InvalidCommitSha)
        );
    }
}
