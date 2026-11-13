/**
 * 项目目录与工作区 API。
 *
 * 统一封装打开、创建、刷新和文件选择；路径校验与项目身份一致性由 Rust 端负责。
 */
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';

import type {
  ProjectHistoryView,
  ProjectStatus,
  ProjectWorkspace,
} from './types';

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

/** 读取应用级最近项目，不读取或修改任何项目业务状态。 */
export async function readProjectHistory() {
  return invoke<ProjectHistoryView>('read_project_history');
}

/** 在校验项目可打开后，把它记录为上次项目并置于最近列表首位。 */
export async function rememberRecentProject(path: string) {
  return invoke<ProjectHistoryView>('remember_recent_project', { path });
}

/** 只从最近列表移除路径，不删除项目目录或数据库。 */
export async function forgetRecentProject(path: string) {
  return invoke<ProjectHistoryView>('forget_recent_project', { path });
}

/** 清空应用级访问历史，不触碰任何项目文件。 */
export async function clearProjectHistory() {
  return invoke<ProjectHistoryView>('clear_project_history');
}
