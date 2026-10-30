import { useState } from 'react';

import * as projectApi from './api/projectApi';
import type { CommandError, ProjectStatus } from './api/types';
import { CreateProjectDialog } from './components/CreateProjectDialog';
import { ErrorBanner } from './components/ErrorBanner';
import { ProjectStatusView } from './components/ProjectStatusView';
import { Sidebar } from './components/Sidebar';
import { WelcomeView } from './components/WelcomeView';

type AppState =
  | { kind: 'welcome' }
  | { kind: 'loading'; label: string }
  | { kind: 'project'; status: ProjectStatus }
  | { kind: 'error'; error: CommandError; previous: ProjectStatus | null };

function normalizeCommandError(error: unknown): CommandError {
  if (typeof error === 'object' && error !== null) {
    const candidate = error as Record<string, unknown>;
    if (
      typeof candidate.code === 'string' &&
      typeof candidate.message === 'string'
    ) {
      return { code: candidate.code, message: candidate.message };
    }
  }

  return { code: 'unknown', message: String(error) };
}

function currentStatus(state: AppState): ProjectStatus | null {
  if (state.kind === 'project') {
    return state.status;
  }
  if (state.kind === 'error') {
    return state.previous;
  }
  return null;
}

function LoadingView({ label }: { label: string }) {
  return (
    <section className="loading-view" role="status" aria-live="polite">
      <div className="loading-view__marker" aria-hidden="true">
        <span />
      </div>
      <p className="eyebrow">SYNCING LOCAL WORKSPACE</p>
      <h1>{label}</h1>
      <p>正在读取项目清单与本地事件状态，请稍候。</p>
    </section>
  );
}

export default function App() {
  const [state, setState] = useState<AppState>({ kind: 'welcome' });
  const [isCreateOpen, setIsCreateOpen] = useState(false);

  async function handleOpenProject() {
    const previous = currentStatus(state);
    setState({ kind: 'loading', label: '正在打开项目' });

    try {
      const path = await projectApi.pickProjectHome();
      if (path === null) {
        setState(
          previous
            ? { kind: 'project', status: previous }
            : { kind: 'welcome' },
        );
        return;
      }

      const status = await projectApi.readProjectStatus(path);
      setState({ kind: 'project', status });
    } catch (error) {
      setState({
        kind: 'error',
        error: normalizeCommandError(error),
        previous,
      });
    }
  }

  async function handleCreateProject(name: string, path: string) {
    const previous = currentStatus(state);
    setState({ kind: 'loading', label: '正在创建项目' });

    try {
      const status = await projectApi.createProject(name, path);
      setIsCreateOpen(false);
      setState({ kind: 'project', status });
    } catch (error) {
      setIsCreateOpen(false);
      setState({
        kind: 'error',
        error: normalizeCommandError(error),
        previous,
      });
    }
  }

  async function handlePickProjectHome() {
    try {
      return await projectApi.pickProjectHome();
    } catch (error) {
      setIsCreateOpen(false);
      setState({
        kind: 'error',
        error: normalizeCommandError(error),
        previous: currentStatus(state),
      });
      return null;
    }
  }

  async function handleRefresh(status: ProjectStatus) {
    setState({ kind: 'loading', label: '正在刷新项目状态' });

    try {
      const refreshed = await projectApi.readProjectStatus(status.rootPath);
      setState({ kind: 'project', status: refreshed });
    } catch (error) {
      setState({
        kind: 'error',
        error: normalizeCommandError(error),
        previous: status,
      });
    }
  }

  function dismissError() {
    if (state.kind !== 'error') {
      return;
    }

    setState(
      state.previous
        ? { kind: 'project', status: state.previous }
        : { kind: 'welcome' },
    );
  }

  const visibleStatus =
    state.kind === 'project'
      ? state.status
      : state.kind === 'error'
        ? state.previous
        : null;

  return (
    <div className="app-shell">
      <Sidebar />

      <main className="workspace">
        <div className="workspace__topline" aria-hidden="true">
          <span>LOCAL-FIRST PROJECT SYSTEM</span>
          <span>PHASE 02 / SHELL</span>
        </div>

        {state.kind === 'error' ? (
          <ErrorBanner error={state.error} onDismiss={dismissError} />
        ) : null}

        <div className="workspace__content">
          {state.kind === 'loading' ? <LoadingView label={state.label} /> : null}

          {state.kind !== 'loading' && visibleStatus ? (
            <ProjectStatusView
              status={visibleStatus}
              onRefresh={() => void handleRefresh(visibleStatus)}
            />
          ) : null}

          {state.kind !== 'loading' && !visibleStatus ? (
            <WelcomeView
              onCreate={() => setIsCreateOpen(true)}
              onOpen={() => void handleOpenProject()}
            />
          ) : null}
        </div>
      </main>

      {isCreateOpen ? (
        <CreateProjectDialog
          submitting={state.kind === 'loading'}
          onCancel={() => setIsCreateOpen(false)}
          onPickDirectory={handlePickProjectHome}
          onCreate={(name, path) => void handleCreateProject(name, path)}
        />
      ) : null}
    </div>
  );
}
