use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ProjectId, PullRequestId, RepositoryId, WorkItemId, WorkReportId, validate_branch_name,
};

pub const MAX_PULL_REQUEST_TITLE_CHARS: usize = 200;
pub const MAX_PULL_REQUEST_URL_CHARS: usize = 2_048;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullRequestState {
    Open,
    Closed,
    Merged,
}

impl PullRequestState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::Merged => "merged",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "open" => Some(Self::Open),
            "closed" => Some(Self::Closed),
            "merged" => Some(Self::Merged),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestRecord {
    pub id: PullRequestId,
    pub project_id: ProjectId,
    pub work_item_id: WorkItemId,
    pub report_id: WorkReportId,
    pub repository_id: RepositoryId,
    pub number: u64,
    pub html_url: String,
    pub title: String,
    pub head_branch: String,
    pub base_branch: String,
    pub state: PullRequestState,
    pub draft: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PullRequestRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        project_id: ProjectId,
        work_item_id: WorkItemId,
        report_id: WorkReportId,
        repository_id: RepositoryId,
        number: u64,
        html_url: String,
        title: String,
        head_branch: String,
        base_branch: String,
        state: PullRequestState,
        draft: bool,
    ) -> Result<Self, PullRequestValidationError> {
        let now = Utc::now();
        let record = Self {
            id: PullRequestId::new(),
            project_id,
            work_item_id,
            report_id,
            repository_id,
            number,
            html_url,
            title,
            head_branch,
            base_branch,
            state,
            draft,
            created_at: now,
            updated_at: now,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn update_state(
        &mut self,
        state: PullRequestState,
        draft: bool,
    ) -> Result<bool, PullRequestValidationError> {
        if self.state == state && self.draft == draft {
            return Ok(false);
        }
        self.state = state;
        self.draft = draft;
        self.updated_at = Utc::now();
        self.validate()?;
        Ok(true)
    }

    pub fn validate(&self) -> Result<(), PullRequestValidationError> {
        if self.number == 0 {
            return Err(PullRequestValidationError::InvalidNumber);
        }
        let url = self.html_url.trim();
        if url.is_empty() {
            return Err(PullRequestValidationError::EmptyUrl);
        }
        if url.chars().count() > MAX_PULL_REQUEST_URL_CHARS {
            return Err(PullRequestValidationError::UrlTooLong);
        }
        if !url.starts_with("https://") {
            return Err(PullRequestValidationError::InsecureUrl);
        }
        if url.chars().any(char::is_control) {
            return Err(PullRequestValidationError::InvalidUrl);
        }
        let title = self.title.trim();
        if title.is_empty() {
            return Err(PullRequestValidationError::EmptyTitle);
        }
        if title.chars().count() > MAX_PULL_REQUEST_TITLE_CHARS {
            return Err(PullRequestValidationError::TitleTooLong);
        }
        validate_branch_name(&self.head_branch)
            .map_err(|_| PullRequestValidationError::InvalidBranch(self.head_branch.clone()))?;
        validate_branch_name(&self.base_branch)
            .map_err(|_| PullRequestValidationError::InvalidBranch(self.base_branch.clone()))?;
        if self.head_branch == self.base_branch {
            return Err(PullRequestValidationError::SameBranch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PullRequestValidationError {
    #[error("pull request number must be greater than zero")]
    InvalidNumber,
    #[error("pull request URL cannot be empty")]
    EmptyUrl,
    #[error("pull request URL must be at most {MAX_PULL_REQUEST_URL_CHARS} characters")]
    UrlTooLong,
    #[error("pull request URL must use HTTPS")]
    InsecureUrl,
    #[error("pull request URL contains invalid characters")]
    InvalidUrl,
    #[error("pull request title cannot be empty")]
    EmptyTitle,
    #[error("pull request title must be at most {MAX_PULL_REQUEST_TITLE_CHARS} characters")]
    TitleTooLong,
    #[error("invalid pull request branch: {0}")]
    InvalidBranch(String),
    #[error("pull request head and base branches must differ")]
    SameBranch,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> PullRequestRecord {
        PullRequestRecord::new(
            ProjectId::new(),
            WorkItemId::new(),
            WorkReportId::new(),
            RepositoryId::new(),
            42,
            "https://github.com/octocat/demo/pull/42".to_owned(),
            "[task] 实现订单".to_owned(),
            "codex/task".to_owned(),
            "main".to_owned(),
            PullRequestState::Open,
            true,
        )
        .expect("pull request must be valid")
    }

    #[test]
    fn rejects_insecure_urls_and_identical_branches() {
        let mut value = record();
        value.html_url = "http://github.com/octocat/demo/pull/42".to_owned();
        assert_eq!(
            value.validate(),
            Err(PullRequestValidationError::InsecureUrl)
        );

        let mut value = record();
        value.base_branch = value.head_branch.clone();
        assert_eq!(
            value.validate(),
            Err(PullRequestValidationError::SameBranch)
        );
    }

    #[test]
    fn state_updates_are_idempotent() {
        let mut value = record();
        assert!(
            !value
                .update_state(PullRequestState::Open, true)
                .expect("state must validate")
        );
        assert!(
            value
                .update_state(PullRequestState::Merged, false)
                .expect("state must validate")
        );
        assert_eq!(value.state, PullRequestState::Merged);
        assert!(value.updated_at >= value.created_at);
    }
}
