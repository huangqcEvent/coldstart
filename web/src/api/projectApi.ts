/**
 * 项目目录与工作区 API。
 *
 * 统一封装打开、创建、刷新和文件选择；路径校验与项目身份一致性由 Rust 端负责。
 */
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';

import type { ProjectStatus, ProjectWorkspace } from './types';

export async function createProject(name: string, path: string) {
  return invoke<ProjectStatus>('create_project', { name, path });
}

export async function readProjectStatus(path: string) {
  return invoke<ProjectStatus>('read_project_status', { path });
}

export async function readProjectWorkspace(path: string) {
  return invoke<ProjectWorkspace>('read_requirements_workspace', { path });
}

export async function pickProjectHome(): Promise<string | null> {
  const selected = await open({
    directory: true,
    multiple: false,
    title: '选择 ColdStart 项目主目录',
  });
  return typeof selected === 'string' ? selected : null;
}
