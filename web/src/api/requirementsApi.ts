import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';

import type { BlueprintContent, ModelConfig, ProjectWorkspace } from './types';

export async function importRequirementDocument(path: string, filePath: string) {
  return invoke<ProjectWorkspace>('import_requirement_document', {
    path,
    filePath,
  });
}

export async function generateBlueprint(path: string, config: ModelConfig) {
  return invoke<ProjectWorkspace>('generate_blueprint', { path, config });
}

export async function saveBlueprintDraft(
  path: string,
  content: BlueprintContent,
) {
  return invoke<ProjectWorkspace>('save_blueprint_draft', { path, content });
}

export async function publishBlueprint(path: string, revisionId: string) {
  return invoke<ProjectWorkspace>('publish_blueprint', {
    path,
    revisionId,
  });
}

export async function resolveInfoRequest(
  path: string,
  infoRequestId: string,
  answer: string,
) {
  return invoke<ProjectWorkspace>('resolve_info_request', {
    path,
    infoRequestId,
    answer,
  });
}

export async function pickRequirementFile(): Promise<string | null> {
  const selected = await open({
    multiple: false,
    directory: false,
    title: '选择需求文档',
    filters: [
      {
        name: '需求文档',
        extensions: ['md', 'markdown', 'txt', 'docx', 'pdf'],
      },
    ],
  });
  return typeof selected === 'string' ? selected : null;
}
