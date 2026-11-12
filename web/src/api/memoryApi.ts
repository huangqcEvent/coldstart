import { invoke } from '@tauri-apps/api/core';

import type { ProjectMemoryPage } from './types';

export interface ReadProjectMemoryOptions {
  beforeCursor: number | null;
  limit: number;
  includeHeartbeats: boolean;
}

export async function readProjectMemory(
  path: string,
  options: ReadProjectMemoryOptions,
) {
  return invoke<ProjectMemoryPage>('read_project_memory', {
    path,
    beforeCursor: options.beforeCursor,
    limit: options.limit,
    includeHeartbeats: options.includeHeartbeats,
  });
}
