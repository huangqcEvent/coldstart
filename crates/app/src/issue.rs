use std::{io::Write, path::PathBuf, sync::Arc};

use chrono::{DateTime, Utc};
use coldstart_adapters::{
    GitHubClient, GitHubError, GitHubIssue, GitHubIssueState, GitHubRepositoryCoordinates,
    GitHubToken, SecretStore, WindowsSecretStore,
};
use coldstart_core::{CredentialKind, Repository, RepositoryAuthMode};
use serde::{Deserialize, Serialize};

use crate::{
    AppError, ProjectWorkspace, context::OpenProject, requirements::import_requirement_file_as,
};

const GITHUB_PROVIDER: &str = "github";
const ISSUE_BODY_PREVIEW_CHARS: usize = 360;

pub struct GitHubIssueService {
    secrets: Arc<dyn SecretStore>,
    github: Option<GitHubClient>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubIssueView {
    pub id: u64,
    pub number: u64,
    pub title: String,
    pub state: String,
    pub body_preview: String,
    pub labels: Vec<String>,
    pub html_url: String,
    pub comments: u64,
    pub updated_at: DateTime<Utc>,
    pub source_reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubIssueListView {
    pub repository_key: String,
    pub repository_name: String,
    pub repository_label: String,
    pub issues: Vec<GitHubIssueView>,
    pub truncated: bool,
}

impl GitHubIssueService {
    pub fn new() -> Self {
        Self::with_dependencies(
            Arc::new(WindowsSecretStore::new()),
            GitHubClient::new().ok(),
        )
    }

    pub fn with_dependencies(secrets: Arc<dyn SecretStore>, github: Option<GitHubClient>) -> Self {
        Self { secrets, github }
    }

    pub fn list_issues(
        &self,
        project_home: PathBuf,
        repository_key: &str,
    ) -> Result<GitHubIssueListView, AppError> {
        let project = OpenProject::open(project_home)?;
        let (repository, coordinates, token) = self.github_context(&project, repository_key)?;
        let client = self.github.as_ref().ok_or(AppError::GitHubNotConfigured)?;
        let list =
            client.list_repository_issues(&token, &coordinates.owner, &coordinates.repository)?;
        let issues = list
            .issues
            .into_iter()
            .map(|issue| issue_view(&coordinates, issue))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(GitHubIssueListView {
            repository_key: repository.key,
            repository_name: repository.name,
            repository_label: format!("{}/{}", coordinates.owner, coordinates.repository),
            issues,
            truncated: list.truncated,
        })
    }

    pub fn import_issue(
        &self,
        project_home: PathBuf,
        repository_key: &str,
        issue_number: u64,
    ) -> Result<ProjectWorkspace, AppError> {
        let project = OpenProject::open(project_home.clone())?;
        let (_repository, coordinates, token) = self.github_context(&project, repository_key)?;
        if issue_number == 0 {
            return Err(GitHubError::InvalidResponse(
                "issue number must be greater than zero".to_owned(),
            )
            .into());
        }
        let source_reference =
            source_reference(&coordinates.owner, &coordinates.repository, issue_number);
        if project
            .database
            .list_artifacts(&project.project.id)?
            .iter()
            .any(|artifact| artifact.import_source == source_reference)
        {
            return Err(AppError::GitHubIssueAlreadyImported {
                repository: format!("{}/{}", coordinates.owner, coordinates.repository),
                number: issue_number,
            });
        }
        let client = self.github.as_ref().ok_or(AppError::GitHubNotConfigured)?;
        let issue = client.repository_issue(
            &token,
            &coordinates.owner,
            &coordinates.repository,
            issue_number,
        )?;
        if issue.number != issue_number {
            return Err(GitHubError::InvalidResponse(
                "GitHub returned a different issue number".to_owned(),
            )
            .into());
        }

        let markdown = issue_markdown(
            &coordinates.owner,
            &coordinates.repository,
            &issue,
            &source_reference,
        );
        let mut temporary = tempfile::Builder::new()
            .prefix("coldstart-github-issue-")
            .suffix(".md")
            .tempfile()?;
        temporary.write_all(markdown.as_bytes())?;
        temporary.flush()?;
        import_requirement_file_as(
            project_home,
            temporary.path(),
            format!("github-issue-{}.md", issue.number),
            source_reference,
        )
    }

    fn github_context(
        &self,
        project: &OpenProject,
        repository_key: &str,
    ) -> Result<(Repository, GitHubRepositoryCoordinates, GitHubToken), AppError> {
        let repository = project
            .database
            .repository_by_key(&project.project.id, repository_key)?
            .ok_or_else(|| AppError::RepositoryNotFound(repository_key.to_owned()))?;
        if repository.auth_mode != RepositoryAuthMode::Credential {
            return Err(AppError::GitHubIssueIneligible(
                repository.key.clone(),
                "仓库必须使用 GitHub OAuth Token 或 Personal Access Token".to_owned(),
            ));
        }
        let credential_id = repository
            .credential_id
            .ok_or(AppError::RepositoryCredentialMissing(repository.id))?;
        let credential = project
            .database
            .credential(&project.project.id, &credential_id)?
            .ok_or(AppError::CredentialNotFound(credential_id))?;
        if !credential
            .provider
            .trim()
            .eq_ignore_ascii_case(GITHUB_PROVIDER)
            || !matches!(
                credential.kind,
                CredentialKind::OAuthToken | CredentialKind::PersonalAccessToken
            )
        {
            return Err(AppError::GitHubIssueIneligible(
                repository.key.clone(),
                "仓库凭据不是 GitHub OAuth Token 或 Personal Access Token".to_owned(),
            ));
        }
        let coordinates = GitHubRepositoryCoordinates::from_remote_url(&repository.remote_url)
            .map_err(|error| {
                AppError::GitHubIssueIneligible(repository.key.clone(), error.to_string())
            })?;
        let token = GitHubToken::new(self.secrets.get(&credential.secret_ref)?)?;
        Ok((repository, coordinates, token))
    }
}

impl Default for GitHubIssueService {
    fn default() -> Self {
        Self::new()
    }
}

fn issue_view(
    coordinates: &GitHubRepositoryCoordinates,
    issue: GitHubIssue,
) -> Result<GitHubIssueView, AppError> {
    let updated_at = DateTime::parse_from_rfc3339(&issue.updated_at)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| GitHubError::InvalidResponse("issue timestamp is not RFC 3339".to_owned()))?;
    Ok(GitHubIssueView {
        id: issue.id,
        number: issue.number,
        title: issue.title,
        state: issue_state(issue.state).to_owned(),
        body_preview: body_preview(issue.body.as_deref().unwrap_or_default()),
        labels: issue.labels,
        html_url: issue.html_url,
        comments: issue.comments,
        updated_at,
        source_reference: source_reference(
            &coordinates.owner,
            &coordinates.repository,
            issue.number,
        ),
    })
}

fn issue_state(state: GitHubIssueState) -> &'static str {
    match state {
        GitHubIssueState::Open => "open",
        GitHubIssueState::Closed => "closed",
    }
}

fn body_preview(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= ISSUE_BODY_PREVIEW_CHARS {
        return trimmed.to_owned();
    }
    let mut preview = trimmed
        .chars()
        .take(ISSUE_BODY_PREVIEW_CHARS)
        .collect::<String>();
    preview.push('…');
    preview
}

fn source_reference(owner: &str, repository: &str, number: u64) -> String {
    format!("github://{owner}/{repository}/issues/{number}")
}

fn issue_markdown(
    owner: &str,
    repository: &str,
    issue: &GitHubIssue,
    source_reference: &str,
) -> String {
    let labels = if issue.labels.is_empty() {
        "无".to_owned()
    } else {
        issue.labels.join("、")
    };
    let state = issue_state(issue.state);
    let body = issue.body.as_deref().unwrap_or("（无正文）");
    format!(
        "# #{number} {title}\n\n- 仓库：{owner}/{repository}\n- 状态：{state}\n- 评论数：{comments}（评论未导入）\n- 更新时间：{updated_at}\n- 链接：{url}\n- 来源标识：{source_reference}\n- 标签：{labels}\n\n> 本来源只包含 GitHub Issue 标题与正文，不包含评论。完整讨论请回到 GitHub 查看。\n\n## 正文\n\n{body}\n",
        number = issue.number,
        title = issue.title,
        comments = issue.comments,
        updated_at = issue.updated_at,
        url = issue.html_url,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_issue_markdown_with_attribution_and_no_comments() {
        let issue = GitHubIssue {
            id: 1,
            number: 7,
            title: "新增导出".to_owned(),
            state: GitHubIssueState::Open,
            body: Some("支持 CSV 导出。".to_owned()),
            labels: vec!["backend".to_owned(), "enhancement".to_owned()],
            html_url: "https://github.com/octocat/demo/issues/7".to_owned(),
            comments: 3,
            updated_at: "2026-11-13T10:00:00Z".to_owned(),
        };
        let markdown = issue_markdown("octocat", "demo", &issue, "github://octocat/demo/issues/7");
        assert!(markdown.contains("# #7 新增导出"));
        assert!(markdown.contains("评论数：3（评论未导入）"));
        assert!(markdown.contains("- 标签：backend、enhancement"));
        assert!(markdown.contains("## 正文\n\n支持 CSV 导出。"));
    }

    #[test]
    fn preview_is_bounded_by_characters() {
        let value = "订".repeat(ISSUE_BODY_PREVIEW_CHARS + 10);
        let preview = body_preview(&value);
        assert_eq!(preview.chars().count(), ISSUE_BODY_PREVIEW_CHARS + 1);
        assert!(preview.ends_with('…'));
    }
}
