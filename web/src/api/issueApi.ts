/**
 * GitHub Issue 需求入口 API。
 *
 * 读取开放或已关闭 Issue 并支持人工导入；浏览器不直接访问 GitHub，也不保存 Token。
 */
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
