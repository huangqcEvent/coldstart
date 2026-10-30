import { invoke } from '@tauri-apps/api/core';

import type { PlanContent, ProjectWorkspace } from './types';

export async function readPlanWorkspace(path: string) {
  return invoke<ProjectWorkspace>('read_plan_workspace', { path });
}

export async function generatePlanDraft(path: string) {
  return invoke<ProjectWorkspace>('generate_plan_draft', { path });
}

export async function savePlanDraft(path: string, content: PlanContent) {
  return invoke<ProjectWorkspace>('save_plan_draft', { path, content });
}

export async function publishPlan(path: string, revisionId: string) {
  return invoke<ProjectWorkspace>('publish_plan', { path, revisionId });
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
