import { invoke } from '@tauri-apps/api/core';

import type { GitHubIssueListView, ProjectWorkspace } from './types';

export async function listGitHubIssues(path: string, repositoryKey: string) {
  return invoke<GitHubIssueListView>('list_github_issues', {
    path,
    repositoryKey,
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
