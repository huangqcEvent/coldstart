import { invoke } from '@tauri-apps/api/core';

import type {
  GitHubIssueListView,
  GitHubIssueListState,
  ProjectWorkspace,
} from './types';

export async function listGitHubIssues(
  path: string,
  repositoryKey: string,
  state: GitHubIssueListState,
) {
  return invoke<GitHubIssueListView>('list_github_issues', {
    path,
    repositoryKey,
    state,
  });
}

export async function importGitHubIssue(
  path: string,
  repositoryKey: string,
  issueNumber: number,
) {
  return invoke<ProjectWorkspace>('import_github_issue', {
    path,
    repositoryKey,
    issueNumber,
  });
}
