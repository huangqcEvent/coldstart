/**
 * GitHub Project 只读概览 API。
 *
 * 项目、字段和条目均由桌面端人工触发读取；本模块不提供写入或后台轮询能力。
 */
import { invoke } from '@tauri-apps/api/core';

import type { GitHubProjectOverviewView } from './types';

export async function listGitHubProjects(path: string, repositoryKey: string) {
  return invoke<GitHubProjectOverviewView>('list_github_projects', {
    path,
    repositoryKey,
  });
}
