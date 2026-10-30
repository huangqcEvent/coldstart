import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import App from './App';
import * as projectApi from './api/projectApi';
import type { ProjectStatus } from './api/types';

vi.mock('./api/projectApi', () => ({
  createProject: vi.fn(),
  readProjectStatus: vi.fn(),
  pickProjectHome: vi.fn(),
}));

const projectPath = String.raw`D:\coldstart-projects\permafrost`;

const initialStatus: ProjectStatus = {
  projectId: 'prj_01JCMQ4W7T3K8Y9F2N6R5D0XAB',
  name: '冻土实验室',
  rootPath: projectPath,
  createdAt: '2026-10-30T08:30:00Z',
  eventCount: 1,
};

const refreshedStatus: ProjectStatus = {
  ...initialStatus,
  createdAt: '2026-10-30T09:45:00Z',
  eventCount: 3,
};

beforeEach(() => {
  vi.mocked(projectApi.createProject).mockReset();
  vi.mocked(projectApi.readProjectStatus).mockReset();
  vi.mocked(projectApi.pickProjectHome).mockReset();
});

describe('ColdStart project screen', () => {
  it('shows the welcome view and the current navigation stage', () => {
    render(<App />);

    expect(screen.getByText('COLDSTART / 冷启动')).toBeInTheDocument();
    expect(
      screen.getByRole('heading', { name: '冷启动' }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: '新建项目' }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: '打开项目' }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: '项目总览' }),
    ).toBeEnabled();
    expect(
      screen.getByRole('button', { name: /需求导入/ }),
    ).toBeDisabled();
  });

  it('creates a project and displays its status', async () => {
    const user = userEvent.setup();
    let resolveCreate!: (status: ProjectStatus) => void;

    vi.mocked(projectApi.pickProjectHome).mockResolvedValue(projectPath);
    vi.mocked(projectApi.createProject).mockImplementation(
      () =>
        new Promise<ProjectStatus>((resolve) => {
          resolveCreate = resolve;
        }),
    );

    render(<App />);
    await user.click(screen.getByRole('button', { name: '新建项目' }));

    const dialog = screen.getByRole('dialog', { name: '新建项目' });
    expect(dialog).toHaveAttribute('aria-modal', 'true');
    await user.type(screen.getByLabelText('项目名称'), initialStatus.name);
    await user.click(screen.getByRole('button', { name: '选择目录' }));

    await waitFor(() => {
      expect(projectApi.pickProjectHome).toHaveBeenCalledTimes(1);
    });
    expect(await screen.findByText(projectPath)).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: '创建项目' }));

    expect(projectApi.createProject).toHaveBeenCalledWith(
      initialStatus.name,
      projectPath,
    );
    expect(await screen.findByText('正在创建项目')).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: '创建中…' }),
    ).toBeDisabled();

    resolveCreate(initialStatus);

    expect(
      await screen.findByRole('heading', { name: initialStatus.name }),
    ).toBeInTheDocument();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(screen.getByText(initialStatus.projectId)).toBeInTheDocument();
    expect(screen.getByText(initialStatus.rootPath)).toBeInTheDocument();
    expect(screen.getByText(initialStatus.createdAt)).toBeInTheDocument();
    expect(screen.getByText('1 个事件')).toBeInTheDocument();
  });

  it('opens a project after selecting its home directory', async () => {
    const user = userEvent.setup();
    let resolveRead!: (status: ProjectStatus) => void;

    vi.mocked(projectApi.pickProjectHome).mockResolvedValue(projectPath);
    vi.mocked(projectApi.readProjectStatus).mockImplementation(
      () =>
        new Promise<ProjectStatus>((resolve) => {
          resolveRead = resolve;
        }),
    );

    render(<App />);
    await user.click(screen.getByRole('button', { name: '打开项目' }));

    expect(projectApi.readProjectStatus).toHaveBeenCalledWith(projectPath);
    expect(await screen.findByText('正在打开项目')).toBeInTheDocument();

    resolveRead(initialStatus);

    expect(
      await screen.findByRole('heading', { name: initialStatus.name }),
    ).toBeInTheDocument();
    expect(screen.getByText(initialStatus.projectId)).toBeInTheDocument();
    expect(screen.getByText(initialStatus.rootPath)).toBeInTheDocument();
  });

  it('shows a closable backend error with its code and message', async () => {
    const user = userEvent.setup();

    vi.mocked(projectApi.pickProjectHome).mockResolvedValue(projectPath);
    vi.mocked(projectApi.readProjectStatus).mockRejectedValue({
      code: 'manifest_missing',
      message: '项目清单不存在',
    });

    render(<App />);
    await user.click(screen.getByRole('button', { name: '打开项目' }));

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('manifest_missing');
    expect(alert).toHaveTextContent('项目清单不存在');

    await user.click(
      within(alert).getByRole('button', { name: '关闭错误提示' }),
    );

    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(
      screen.getByRole('heading', { name: '冷启动' }),
    ).toBeInTheDocument();
  });

  it('refreshes the project and renders the updated time and event count', async () => {
    const user = userEvent.setup();

    vi.mocked(projectApi.pickProjectHome).mockResolvedValue(projectPath);
    vi.mocked(projectApi.readProjectStatus)
      .mockResolvedValueOnce(initialStatus)
      .mockResolvedValueOnce(refreshedStatus);

    render(<App />);
    await user.click(screen.getByRole('button', { name: '打开项目' }));
    await screen.findByRole('heading', { name: initialStatus.name });

    await user.click(screen.getByRole('button', { name: '刷新状态' }));

    expect(await screen.findByText('3 个事件')).toBeInTheDocument();
    expect(screen.getByText(refreshedStatus.createdAt)).toBeInTheDocument();
    expect(screen.queryByText(initialStatus.createdAt)).not.toBeInTheDocument();
    expect(projectApi.readProjectStatus).toHaveBeenCalledTimes(2);
    expect(projectApi.readProjectStatus).toHaveBeenLastCalledWith(projectPath);
  });

  it('keeps the previous project visible when a refresh fails', async () => {
    const user = userEvent.setup();

    vi.mocked(projectApi.pickProjectHome).mockResolvedValue(projectPath);
    vi.mocked(projectApi.readProjectStatus)
      .mockResolvedValueOnce(initialStatus)
      .mockRejectedValueOnce({
        code: 'storage',
        message: '数据库不可读',
      });

    render(<App />);
    await user.click(screen.getByRole('button', { name: '打开项目' }));
    await screen.findByRole('heading', { name: initialStatus.name });

    await user.click(screen.getByRole('button', { name: '刷新状态' }));

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('storage');
    expect(alert).toHaveTextContent('数据库不可读');
    expect(
      screen.getByRole('heading', { name: initialStatus.name }),
    ).toBeInTheDocument();

    await user.click(
      within(alert).getByRole('button', { name: '关闭错误提示' }),
    );
    expect(
      screen.getByRole('heading', { name: initialStatus.name }),
    ).toBeInTheDocument();
  });

  it('normalizes unknown backend failures', async () => {
    const user = userEvent.setup();

    vi.mocked(projectApi.pickProjectHome).mockResolvedValue(projectPath);
    vi.mocked(projectApi.readProjectStatus).mockRejectedValue(
      new Error('IPC unavailable'),
    );

    render(<App />);
    await user.click(screen.getByRole('button', { name: '打开项目' }));

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('unknown');
    expect(alert).toHaveTextContent('Error: IPC unavailable');
  });

  it('cancels the create dialog without calling the API', async () => {
    const user = userEvent.setup();

    render(<App />);
    await user.click(screen.getByRole('button', { name: '新建项目' }));
    await user.click(screen.getByRole('button', { name: '取消' }));

    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(projectApi.createProject).not.toHaveBeenCalled();
    expect(projectApi.pickProjectHome).not.toHaveBeenCalled();
  });
});
