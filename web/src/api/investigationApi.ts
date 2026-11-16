/**
 * 历史项目只读代码调查 API。
 *
 * 桌面端只创建调查请求、确认或驳回 Codex 的结构化报告；源码读取和证据采集
 * 始终发生在已映射仓库内，不通过前端传输源码正文或 Secret。
 */
import { invoke } from '@tauri-apps/api/core';

import type { ProjectWorkspace } from './types';

export async function createCodeInvestigation(
  path: string,
  repositoryKey: string,
  baseRef: string | null,
  question: string,
) {
  return invoke<ProjectWorkspace>('create_code_investigation', {
    path,
    repositoryKey,
    baseRef,
    question,
  });
}

export async function confirmCodeInvestigation(
  path: string,
  investigationId: string,
) {
  return invoke<ProjectWorkspace>('confirm_code_investigation', {
    path,
    investigationId,
  });
}

export async function rejectCodeInvestigation(
  path: string,
  investigationId: string,
  reason: string,
) {
  return invoke<ProjectWorkspace>('reject_code_investigation', {
    path,
    investigationId,
    reason,
  });
}
