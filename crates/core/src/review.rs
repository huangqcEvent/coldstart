use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ArtifactFormat, CredentialId, ProjectId, ReviewStandardId, ReviewStandardRevisionId, sha256_hex,
};

pub const REVIEW_SNAPSHOT_SCHEMA_VERSION: u32 = 1;
pub const REVIEW_CONSENT_VERSION: u32 = 1;
pub const MAX_REVIEW_STANDARDS: usize = 20;
pub const MAX_REVIEW_STANDARD_CHARS: usize = 60_000;
pub const MAX_REVIEW_DIFF_CHARS: usize = 160_000;
pub const MAX_REVIEW_FILES: usize = 1_000;
pub const MAX_REVIEW_FINDINGS: usize = 100;
pub const MAX_REVIEW_SUMMARY_CHARS: usize = 4_000;
pub const MAX_REVIEW_FINDING_MESSAGE_CHARS: usize = 2_000;
pub const MAX_REVIEW_FINDING_PATH_CHARS: usize = 500;
pub const REVIEW_TIMEOUT_SECONDS: u64 = 180;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewSeverity {
    Critical,
    High,
    Medium,
    Low,
}

impl ReviewSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "critical" => Some(Self::Critical),
            "high" => Some(Self::High),
            "medium" => Some(Self::Medium),
            "low" => Some(Self::Low),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewCategory {
    Security,
    Correctness,
    Tests,
    Maintainability,
    Style,
    Performance,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "repositoryKey", rename_all = "snake_case")]
pub enum ReviewScope {
    Project,
    Repository(String),
}

impl ReviewScope {
    pub fn repository_key(&self) -> Option<&str> {
        match self {
            Self::Project => None,
            Self::Repository(repository_key) => Some(repository_key),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStandardSource {
    Upload,
    Manual,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    #[default]
    NotRequired,
    Running,
    Passed,
    Blocked,
    Failed,
    Waived,
    Interrupted,
}

impl ReviewStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotRequired => "not_required",
            Self::Running => "running",
            Self::Passed => "passed",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
            Self::Waived => "waived",
            Self::Interrupted => "interrupted",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "not_required" => Some(Self::NotRequired),
            "running" => Some(Self::Running),
            "passed" => Some(Self::Passed),
            "blocked" => Some(Self::Blocked),
            "failed" => Some(Self::Failed),
            "waived" => Some(Self::Waived),
            "interrupted" => Some(Self::Interrupted),
            _ => None,
        }
    }

    pub fn permits_completion(self) -> bool {
        matches!(self, Self::NotRequired | Self::Passed | Self::Waived)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewConfigurationInput {
    pub enabled: bool,
    pub base_url: String,
    pub model: String,
    pub credential_id: Option<CredentialId>,
    pub blocking_severities: Vec<ReviewSeverity>,
    pub consent_confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewConfiguration {
    pub project_id: ProjectId,
    pub enabled: bool,
    pub base_url: String,
    pub model: String,
    pub credential_id: Option<CredentialId>,
    pub blocking_severities: Vec<ReviewSeverity>,
    pub consent_fingerprint: Option<String>,
    pub updated_at: DateTime<Utc>,
}

impl ReviewConfiguration {
    pub fn from_input(
        project_id: ProjectId,
        input: ReviewConfigurationInput,
    ) -> Result<Self, ReviewValidationError> {
        let configuration = Self {
            project_id,
            enabled: input.enabled,
            base_url: normalize_base_url(&input.base_url),
            model: input.model.trim().to_owned(),
            credential_id: input.credential_id,
            blocking_severities: normalize_blocking_severities(input.blocking_severities)?,
            consent_fingerprint: input.consent_confirmed.then(|| {
                consent_fingerprint_for(
                    &normalize_base_url(&input.base_url),
                    input.model.trim(),
                    input.credential_id,
                )
            }),
            updated_at: Utc::now(),
        };
        configuration.validate()?;
        Ok(configuration)
    }

    pub fn validate(&self) -> Result<(), ReviewValidationError> {
        normalize_blocking_severities(self.blocking_severities.clone())?;
        if !self.enabled {
            return Ok(());
        }
        if !(self.base_url.starts_with("http://") || self.base_url.starts_with("https://")) {
            return Err(ReviewValidationError::InvalidBaseUrl(self.base_url.clone()));
        }
        if self.model.trim().is_empty() {
            return Err(ReviewValidationError::EmptyModel);
        }
        Ok(())
    }

    pub fn expected_consent_fingerprint(&self) -> String {
        consent_fingerprint_for(&self.base_url, &self.model, self.credential_id)
    }

    pub fn consent_fingerprint(&self) -> Option<&str> {
        self.consent_fingerprint.as_deref()
    }

    pub fn has_valid_consent(&self) -> bool {
        self.consent_fingerprint.as_deref() == Some(self.expected_consent_fingerprint().as_str())
    }
}

pub fn consent_fingerprint_for(
    base_url: &str,
    model: &str,
    credential_id: Option<CredentialId>,
) -> String {
    let material = format!(
        "coldstart:review-consent:{REVIEW_CONSENT_VERSION}:{}:{}:{}",
        normalize_base_url(base_url),
        model.trim(),
        credential_id
            .map(|credential_id| credential_id.to_string())
            .unwrap_or_default()
    );
    sha256_hex(material.as_bytes())
}

pub fn default_blocking_severities() -> Vec<ReviewSeverity> {
    vec![ReviewSeverity::Critical, ReviewSeverity::High]
}

pub fn normalize_blocking_severities(
    values: Vec<ReviewSeverity>,
) -> Result<Vec<ReviewSeverity>, ReviewValidationError> {
    let values = if values.is_empty() {
        default_blocking_severities()
    } else {
        values
    };
    let mut unique = BTreeSet::new();
    unique.extend(values);
    Ok(unique.into_iter().collect())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewConfigurationSnapshot {
    pub enabled: bool,
    pub base_url: String,
    pub model: String,
    pub credential_id: Option<CredentialId>,
    pub blocking_severities: Vec<ReviewSeverity>,
    pub consent_fingerprint: String,
}

impl ReviewConfigurationSnapshot {
    pub fn from_configuration(configuration: &ReviewConfiguration) -> Self {
        Self {
            enabled: configuration.enabled,
            base_url: configuration.base_url.clone(),
            model: configuration.model.clone(),
            credential_id: configuration.credential_id,
            blocking_severities: configuration.blocking_severities.clone(),
            consent_fingerprint: configuration
                .consent_fingerprint
                .clone()
                .unwrap_or_else(|| configuration.expected_consent_fingerprint()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewStandardRevision {
    pub id: ReviewStandardRevisionId,
    pub standard_id: ReviewStandardId,
    pub project_id: ProjectId,
    pub revision_number: u64,
    pub source: ReviewStandardSource,
    pub file_name: String,
    pub format: ArtifactFormat,
    pub content_hash: String,
    pub text_hash: String,
    pub character_count: usize,
    pub storage_relative_path: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewStandard {
    pub id: ReviewStandardId,
    pub project_id: ProjectId,
    pub name: String,
    pub scope: ReviewScope,
    pub enabled: bool,
    pub position: u32,
    pub current_revision: ReviewStandardRevision,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewStandardSnapshot {
    pub standard_id: ReviewStandardId,
    pub revision_id: ReviewStandardRevisionId,
    pub name: String,
    pub scope: ReviewScope,
    pub position: u32,
    pub content_hash: String,
    pub character_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSnapshot {
    pub schema_version: u32,
    pub repository_key: String,
    pub base_commit: String,
    pub configuration: ReviewConfigurationSnapshot,
    pub standards: Vec<ReviewStandardSnapshot>,
    pub created_at: DateTime<Utc>,
}

impl ReviewSnapshot {
    pub fn new(
        repository_key: impl Into<String>,
        base_commit: impl Into<String>,
        configuration: ReviewConfigurationSnapshot,
        standards: Vec<ReviewStandardSnapshot>,
    ) -> Result<Self, ReviewValidationError> {
        let snapshot = Self {
            schema_version: REVIEW_SNAPSHOT_SCHEMA_VERSION,
            repository_key: repository_key.into().trim().to_owned(),
            base_commit: base_commit.into().trim().to_owned(),
            configuration,
            standards,
            created_at: Utc::now(),
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn validate(&self) -> Result<(), ReviewValidationError> {
        if self.schema_version != REVIEW_SNAPSHOT_SCHEMA_VERSION {
            return Err(ReviewValidationError::UnsupportedSnapshotVersion(
                self.schema_version,
            ));
        }
        if !crate::integration::REPOSITORY_KEYS.contains(&self.repository_key.as_str()) {
            return Err(ReviewValidationError::InvalidRepositoryKey(
                self.repository_key.clone(),
            ));
        }
        if !is_commit_sha(&self.base_commit) {
            return Err(ReviewValidationError::InvalidBaseCommit(
                self.base_commit.clone(),
            ));
        }
        if !self.configuration.enabled {
            return Err(ReviewValidationError::SnapshotReviewDisabled);
        }
        if !(self.configuration.base_url.starts_with("http://")
            || self.configuration.base_url.starts_with("https://"))
        {
            return Err(ReviewValidationError::InvalidBaseUrl(
                self.configuration.base_url.clone(),
            ));
        }
        if self.configuration.model.trim().is_empty() {
            return Err(ReviewValidationError::EmptyModel);
        }
        if self.configuration.consent_fingerprint.is_empty() {
            return Err(ReviewValidationError::MissingConsent);
        }
        validate_standards(&self.standards)?;
        Ok(())
    }

    pub fn content_hash(&self) -> String {
        let mut canonical = self.clone();
        canonical.created_at = DateTime::<Utc>::UNIX_EPOCH;
        let json = serde_json::to_vec(&canonical).expect("review snapshot must serialize");
        sha256_hex(&json)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewStandardReferenceView {
    pub reference_id: String,
    pub standard_id: ReviewStandardId,
    pub revision_id: ReviewStandardRevisionId,
    pub name: String,
    pub scope: ReviewScope,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewContextView {
    pub enabled: bool,
    pub blocking_severities: Vec<ReviewSeverity>,
    pub standards: Vec<ReviewStandardReferenceView>,
}

impl ReviewContextView {
    pub fn from_snapshot(snapshot: &ReviewSnapshot) -> Self {
        Self {
            enabled: snapshot.configuration.enabled,
            blocking_severities: snapshot.configuration.blocking_severities.clone(),
            standards: snapshot
                .standards
                .iter()
                .map(|standard| ReviewStandardReferenceView {
                    reference_id: format!("review-standard:{}", standard.standard_id),
                    standard_id: standard.standard_id,
                    revision_id: standard.revision_id,
                    name: standard.name.clone(),
                    scope: standard.scope.clone(),
                    content_hash: standard.content_hash.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFinding {
    pub severity: ReviewSeverity,
    pub category: ReviewCategory,
    pub file: String,
    pub line: Option<u32>,
    pub message: String,
    pub recommendation: String,
    pub standard: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFailure {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewResult {
    pub snapshot_hash: String,
    pub status: ReviewStatus,
    pub summary: String,
    pub findings: Vec<ReviewFinding>,
    pub model: String,
    pub failure: Option<ReviewFailure>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewWaiver {
    pub reason: String,
    pub waived_by: String,
    pub waived_at: DateTime<Utc>,
    pub findings_hash: String,
    pub standards_hash: String,
}

pub fn validate_review_summary(value: &str) -> Result<String, ReviewValidationError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ReviewValidationError::EmptySummary);
    }
    if value.chars().count() > MAX_REVIEW_SUMMARY_CHARS {
        return Err(ReviewValidationError::SummaryTooLong);
    }
    Ok(value.to_owned())
}

pub fn validate_review_findings(
    findings: Vec<ReviewFinding>,
    changed_files: &[String],
) -> Result<Vec<ReviewFinding>, ReviewValidationError> {
    if findings.len() > MAX_REVIEW_FINDINGS {
        return Err(ReviewValidationError::TooManyFindings(findings.len()));
    }
    let changed_files = changed_files
        .iter()
        .map(|path| path.trim().replace('\\', "/"))
        .collect::<BTreeSet<_>>();
    let mut validated = Vec::with_capacity(findings.len());
    for mut finding in findings {
        if finding.message.trim().is_empty() {
            return Err(ReviewValidationError::EmptyFindingMessage);
        }
        if finding.message.chars().count() > MAX_REVIEW_FINDING_MESSAGE_CHARS {
            return Err(ReviewValidationError::FindingMessageTooLong);
        }
        if finding.recommendation.chars().count() > MAX_REVIEW_FINDING_MESSAGE_CHARS {
            return Err(ReviewValidationError::FindingRecommendationTooLong);
        }
        if finding.line == Some(0) {
            return Err(ReviewValidationError::InvalidFindingLine);
        }
        let path = normalize_finding_path(&finding.file)?;
        if !changed_files.contains(&path) {
            return Err(ReviewValidationError::FindingFileNotInDiff(path));
        }
        finding.file = path;
        finding.message = finding.message.trim().to_owned();
        finding.recommendation = finding.recommendation.trim().to_owned();
        finding.standard = finding
            .standard
            .map(|standard| standard.trim().to_owned())
            .filter(|standard| !standard.is_empty());
        validated.push(finding);
    }
    Ok(validated)
}

pub fn has_blocking_findings(
    findings: &[ReviewFinding],
    blocking_severities: &[ReviewSeverity],
) -> bool {
    findings
        .iter()
        .any(|finding| blocking_severities.contains(&finding.severity))
}

pub fn findings_hash(findings: &[ReviewFinding]) -> String {
    let json = serde_json::to_vec(findings).expect("review findings must serialize");
    sha256_hex(&json)
}

fn validate_standards(standards: &[ReviewStandardSnapshot]) -> Result<(), ReviewValidationError> {
    if standards.len() > MAX_REVIEW_STANDARDS {
        return Err(ReviewValidationError::TooManyStandards(standards.len()));
    }
    let total_chars = standards
        .iter()
        .map(|standard| standard.character_count)
        .sum::<usize>();
    if total_chars > MAX_REVIEW_STANDARD_CHARS {
        return Err(ReviewValidationError::StandardsTooLarge(total_chars));
    }
    let mut positions = BTreeSet::new();
    for standard in standards {
        if standard.name.trim().is_empty() {
            return Err(ReviewValidationError::EmptyStandardName);
        }
        if !positions.insert(standard.position) {
            return Err(ReviewValidationError::DuplicateStandardPosition(
                standard.position,
            ));
        }
        if let ReviewScope::Repository(repository_key) = &standard.scope
            && !crate::integration::REPOSITORY_KEYS.contains(&repository_key.as_str())
        {
            return Err(ReviewValidationError::InvalidRepositoryKey(
                repository_key.clone(),
            ));
        }
    }
    Ok(())
}

fn normalize_finding_path(value: &str) -> Result<String, ReviewValidationError> {
    let path = value.trim().replace('\\', "/");
    if path.is_empty()
        || path.chars().count() > MAX_REVIEW_FINDING_PATH_CHARS
        || path.starts_with('/')
        || path.starts_with("//")
        || path.as_bytes().get(1) == Some(&b':')
        || path.split('/').any(|part| part == ".." || part.is_empty())
    {
        return Err(ReviewValidationError::InvalidFindingPath(value.to_owned()));
    }
    Ok(path)
}

fn normalize_base_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_owned()
}

fn is_commit_sha(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReviewValidationError {
    #[error("review API base URL is invalid: {0}")]
    InvalidBaseUrl(String),
    #[error("review model cannot be empty")]
    EmptyModel,
    #[error("review summary cannot be empty")]
    EmptySummary,
    #[error("review summary exceeds {MAX_REVIEW_SUMMARY_CHARS} characters")]
    SummaryTooLong,
    #[error("review findings exceed the limit: {0}")]
    TooManyFindings(usize),
    #[error("review finding message cannot be empty")]
    EmptyFindingMessage,
    #[error("review finding message exceeds {MAX_REVIEW_FINDING_MESSAGE_CHARS} characters")]
    FindingMessageTooLong,
    #[error("review finding recommendation exceeds {MAX_REVIEW_FINDING_MESSAGE_CHARS} characters")]
    FindingRecommendationTooLong,
    #[error("review finding path is invalid: {0}")]
    InvalidFindingPath(String),
    #[error("review finding file is not part of the diff: {0}")]
    FindingFileNotInDiff(String),
    #[error("review finding line must be a positive integer")]
    InvalidFindingLine,
    #[error("review standards exceed the limit: {0}")]
    TooManyStandards(usize),
    #[error("review standard text exceeds the limit: {0}")]
    StandardsTooLarge(usize),
    #[error("review standard name cannot be empty")]
    EmptyStandardName,
    #[error("review standard position is duplicated: {0}")]
    DuplicateStandardPosition(u32),
    #[error("review standard targets an invalid repository key: {0}")]
    InvalidRepositoryKey(String),
    #[error("review snapshot schema version is unsupported: {0}")]
    UnsupportedSnapshotVersion(u32),
    #[error("review snapshot has an invalid base commit: {0}")]
    InvalidBaseCommit(String),
    #[error("review snapshot cannot be created while review is disabled")]
    SnapshotReviewDisabled,
    #[error("review consent is required")]
    MissingConsent,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocking_severity_defaults_are_stable_and_deduplicated() {
        assert_eq!(
            normalize_blocking_severities(vec![
                ReviewSeverity::High,
                ReviewSeverity::High,
                ReviewSeverity::Critical,
            ])
            .expect("severities"),
            default_blocking_severities()
        );
    }

    #[test]
    fn repository_scope_reports_its_repository_key() {
        assert_eq!(ReviewScope::Project.repository_key(), None);
        assert_eq!(
            ReviewScope::Repository("web".to_owned()).repository_key(),
            Some("web")
        );
    }
}
