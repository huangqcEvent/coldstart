//! GitHub Project v2 只读概览。
//!
//! 读取仓库关联的 Projects、有限条目和单选字段，供人工查看外部协作状态；不写
//! Project、不导入条目，也不向 Agent 暴露 Project 数据。

use std::{path::PathBuf, sync::Arc};

use chrono::{DateTime, Utc};
use coldstart_adapters::{
    GitHubClient, GitHubError, GitHubProject, GitHubProjectItemKind, GitHubProjectItemState,
    SecretStore, WindowsSecretStore,
};
use serde::{Deserialize, Serialize};

use crate::{AppError, context::OpenProject, github_access::resolve_github_repository_access};

pub struct GitHubProjectService {
    secrets: Arc<dyn SecretStore>,
    github: Option<GitHubClient>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubProjectFieldView {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubProjectItemView {
    pub id: String,
    pub kind: String,
    pub number: Option<u64>,
    pub title: String,
    pub state: Option<String>,
    pub html_url: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
    pub fields: Vec<GitHubProjectFieldView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubProjectView {
    pub id: String,
    pub number: u64,
    pub title: String,
    pub html_url: String,
    pub closed: bool,
    pub updated_at: DateTime<Utc>,
    pub total_items: u64,
    pub items: Vec<GitHubProjectItemView>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubProjectOverviewView {
    pub repository_key: String,
    pub repository_name: String,
    pub repository_label: String,
    pub projects: Vec<GitHubProjectView>,
    pub truncated: bool,
}

impl GitHubProjectService {
    pub fn new() -> Self {
        Self::with_dependencies(
            Arc::new(WindowsSecretStore::new()),
            GitHubClient::new().ok(),
        )
    }

    pub fn with_dependencies(secrets: Arc<dyn SecretStore>, github: Option<GitHubClient>) -> Self {
        Self { secrets, github }
    }

    pub fn list_projects(
        &self,
        project_home: PathBuf,
        repository_key: &str,
    ) -> Result<GitHubProjectOverviewView, AppError> {
        let project = OpenProject::open(project_home)?;
        let access = resolve_github_repository_access(
            &project,
            repository_key,
            self.secrets.as_ref(),
            self.github.as_ref(),
            AppError::GitHubProjectIneligible,
        )?;
        let client = self.github.as_ref().ok_or(AppError::GitHubNotConfigured)?;
        let overview = client.list_repository_projects(
            &access.token,
            &access.coordinates.owner,
            &access.coordinates.repository,
        )?;
        let projects = overview
            .projects
            .into_iter()
            .map(project_view)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(GitHubProjectOverviewView {
            repository_key: access.repository.key,
            repository_name: access.repository.name,
            repository_label: format!(
                "{}/{}",
                access.coordinates.owner, access.coordinates.repository
            ),
            projects,
            truncated: overview.truncated,
        })
    }
}

impl Default for GitHubProjectService {
    fn default() -> Self {
        Self::new()
    }
}

fn project_view(project: GitHubProject) -> Result<GitHubProjectView, AppError> {
    let updated_at = parse_timestamp(&project.updated_at, "project updated timestamp")?;
    let items = project
        .items
        .into_iter()
        .map(|item| {
            Ok(GitHubProjectItemView {
                id: item.id,
                kind: item_kind(item.kind).to_owned(),
                number: item.number,
                title: item.title,
                state: item.state.map(item_state).map(str::to_owned),
                html_url: item.html_url,
                updated_at: item
                    .updated_at
                    .as_deref()
                    .map(|value| parse_timestamp(value, "project item updated timestamp"))
                    .transpose()?,
                fields: item
                    .fields
                    .into_iter()
                    .map(|field| GitHubProjectFieldView {
                        name: field.name,
                        value: field.value,
                    })
                    .collect(),
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(GitHubProjectView {
        id: project.id,
        number: project.number,
        title: project.title,
        html_url: project.html_url,
        closed: project.closed,
        updated_at,
        total_items: project.total_items,
        items,
        truncated: project.truncated,
    })
}

fn parse_timestamp(value: &str, field: &str) -> Result<DateTime<Utc>, AppError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| GitHubError::InvalidResponse(format!("{field} is not RFC 3339")).into())
}

fn item_kind(kind: GitHubProjectItemKind) -> &'static str {
    match kind {
        GitHubProjectItemKind::Issue => "issue",
        GitHubProjectItemKind::PullRequest => "pull_request",
        GitHubProjectItemKind::DraftIssue => "draft_issue",
        GitHubProjectItemKind::Unavailable => "unavailable",
    }
}

fn item_state(state: GitHubProjectItemState) -> &'static str {
    match state {
        GitHubProjectItemState::Open => "open",
        GitHubProjectItemState::Closed => "closed",
        GitHubProjectItemState::Merged => "merged",
    }
}
