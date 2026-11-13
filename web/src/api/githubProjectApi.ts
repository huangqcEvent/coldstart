import { invoke } from '@tauri-apps/api/core';

import type { GitHubProjectOverviewView } from './types';

export async function listGitHubProjects(path: string, repositoryKey: string) {
  return invoke<GitHubProjectOverviewView>('list_github_projects', {
    path,
    repositoryKey,
  });
}
