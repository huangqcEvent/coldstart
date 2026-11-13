/**
 * 计划与任务操作 API。
 *
 * 只负责把用户明确动作传给 Rust 服务；领取、报告、验证与 Review 状态机不在前端复制。
 */
import { invoke } from '@tauri-apps/api/core';

import type { PlanContent, PlanPublishPreview, ProjectWorkspace } from './types';

export async function readPlanWorkspace(path: string) {
  return invoke<ProjectWorkspace>('read_plan_workspace', { path });
}

export async function generatePlanDraft(path: string) {
  return invoke<ProjectWorkspace>('generate_plan_draft', { path });
}

export async function savePlanDraft(path: string, content: PlanContent) {
  return invoke<ProjectWorkspace>('save_plan_draft', { path, content });
}

export async function previewPlanPublish(path: string, revisionId: string) {
  return invoke<PlanPublishPreview>('preview_plan_publish', {
    path,
    revisionId,
  });
}

export async function publishPlan(
  path: string,
  revisionId: string,
  confirmed: boolean,
) {
  return invoke<ProjectWorkspace>('publish_plan', {
    path,
    revisionId,
    confirmed,
  });
}

export async function completeHumanWork(
  path: string,
  workItemId: string,
  note: string,
  accepted: boolean,
) {
  return invoke<ProjectWorkspace>('complete_human_work', {
    path,
    workItemId,
    note,
    accepted,
  });
}
