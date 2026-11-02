import { useState } from 'react';

import * as planApi from './api/planApi';
import * as projectApi from './api/projectApi';
import * as requirementsApi from './api/requirementsApi';
import type {
  BlueprintContent,
  CommandError,
  ModelConfig,
  PlanContent,
  PlanPublishPreview,
  ProjectWorkspace,
} from './api/types';
import { AppSidebar, type WorkspaceView } from './components/AppSidebar';
import { BlueprintView } from './components/BlueprintView';
import { CreateProjectDialog } from './components/CreateProjectDialog';
import { ErrorBanner } from './components/ErrorBanner';
import { InboxView } from './components/InboxView';
import { IntegrationView } from './components/IntegrationView';
import { PlanView } from './components/PlanView';
import { ProjectOverview } from './components/ProjectOverview';
import { ReleaseCenterView } from './components/ReleaseCenterView';
import { RequirementsView } from './components/RequirementsView';
import { ReviewView } from './components/ReviewView';
import { WelcomeView } from './components/WelcomeView';

function normalizeCommandError(error: unknown): CommandError {
  if (typeof error === 'object' && error !== null) {
    const candidate = error as Record<string, unknown>;
    if (typeof candidate.code === 'string' && typeof candidate.message === 'string') {
      return { code: candidate.code, message: candidate.message };
    }
  }

  return { code: 'unknown', message: String(error) };
}

function LoadingView({ label }: { label: string }) {
  return (
    <section className="loading-view" role="status" aria-live="polite">
      <div className="loading-view__marker" aria-hidden="true">
        <span />
      </div>
      <p className="eyebrow">SYNCING LOCAL WORKSPACE</p>
      <h1>{label}</h1>
      <p>正在读取项目清单、来源文档与本地蓝图状态，请稍候。</p>
    </section>
  );
}

export default function App() {
  const [workspace, setWorkspace] = useState<ProjectWorkspace | null>(null);
  const [view, setView] = useState<WorkspaceView>('overview');
  const [busyLabel, setBusyLabel] = useState<string | null>(null);
  const [error, setError] = useState<CommandError | null>(null);
  const [isCreateOpen, setIsCreateOpen] = useState(false);

  async function loadProject(path: string, label: string) {
    setBusyLabel(label);
    setError(null);
    try {
      const [status, loadedWorkspace] = await Promise.all([
        projectApi.readProjectStatus(path),
        projectApi.readProjectWorkspace(path),
      ]);
      setWorkspace({ ...loadedWorkspace, status });
      setView('overview');
      return true;
    } catch (loadError) {
      setError(normalizeCommandError(loadError));
      return false;
    } finally {
      setBusyLabel(null);
    }
  }

  async function handleOpenProject() {
    if (busyLabel) {
      return;
    }

    let path: string | null;
    try {
      path = await projectApi.pickProjectHome();
    } catch (pickError) {
      setError(normalizeCommandError(pickError));
      return;
    }
    if (path !== null) {
      await loadProject(path, '正在打开项目');
    }
  }

  async function handleCreateProject(name: string, path: string) {
    if (busyLabel) {
      return;
    }

    setBusyLabel('正在创建项目');
    setError(null);
    try {
      const status = await projectApi.createProject(name, path);
      const loadedWorkspace = await projectApi.readProjectWorkspace(path);
      setWorkspace({ ...loadedWorkspace, status });
      setView('overview');
      setIsCreateOpen(false);
    } catch (createError) {
      setIsCreateOpen(false);
      setError(normalizeCommandError(createError));
    } finally {
      setBusyLabel(null);
    }
  }

  async function handlePickProjectHome() {
    try {
      return await projectApi.pickProjectHome();
    } catch (pickError) {
      setError(normalizeCommandError(pickError));
      return null;
    }
  }

  async function handleRefresh() {
    if (!workspace || busyLabel) {
      return;
    }
    const path = workspace.status.rootPath;
    setBusyLabel('正在刷新项目状态');
    setError(null);
    try {
      const [status, refreshed] = await Promise.all([
        projectApi.readProjectStatus(path),
        projectApi.readProjectWorkspace(path),
      ]);
      setWorkspace({ ...refreshed, status });
    } catch (refreshError) {
      setError(normalizeCommandError(refreshError));
    } finally {
      setBusyLabel(null);
    }
  }

  async function runWorkspaceAction(
    label: string,
    operation: () => Promise<ProjectWorkspace>,
  ) {
    if (!workspace || busyLabel) {
      return false;
    }

    setBusyLabel(label);
    setError(null);
    try {
      setWorkspace(await operation());
      return true;
    } catch (actionError) {
      setError(normalizeCommandError(actionError));
      return false;
    } finally {
      setBusyLabel(null);
    }
  }

  async function handleImport() {
    if (!workspace || busyLabel) {
      return;
    }
    const path = workspace.status.rootPath;
    let filePath: string | null;
    try {
      filePath = await requirementsApi.pickRequirementFile();
    } catch (pickError) {
      setError(normalizeCommandError(pickError));
      return;
    }
    if (filePath === null) {
      return;
    }
    await runWorkspaceAction('正在导入需求文档', () =>
      requirementsApi.importRequirementDocument(path, filePath),
    );
  }

  async function handleGenerate(config: ModelConfig) {
    if (!workspace) {
      return;
    }
    const path = workspace.status.rootPath;
    await runWorkspaceAction('正在生成蓝图草稿', () =>
      requirementsApi.generateBlueprint(path, config),
    );
  }

  async function handleSaveBlueprint(content: BlueprintContent) {
    if (!workspace) {
      return false;
    }
    const path = workspace.status.rootPath;
    return runWorkspaceAction('正在保存蓝图草稿', () =>
      requirementsApi.saveBlueprintDraft(path, content),
    );
  }

  async function handlePublishBlueprint(revisionId: string) {
    if (!workspace) {
      return false;
    }
    const path = workspace.status.rootPath;
    return runWorkspaceAction('正在发布蓝图', () =>
      requirementsApi.publishBlueprint(path, revisionId),
    );
  }

  async function handleResolveInfoRequest(requestId: string, answer: string) {
    if (!workspace) {
      return false;
    }
    const path = workspace.status.rootPath;
    return runWorkspaceAction('正在提交 Inbox 回答', () =>
      requirementsApi.resolveInfoRequest(path, requestId, answer),
    );
  }

  async function handleGeneratePlan() {
    if (!workspace) {
      return false;
    }
    const path = workspace.status.rootPath;
    return runWorkspaceAction('正在编译开发计划', () =>
      planApi.generatePlanDraft(path),
    );
  }

  async function handleSavePlan(content: PlanContent) {
    if (!workspace) {
      return false;
    }
    const path = workspace.status.rootPath;
    return runWorkspaceAction('正在保存计划草稿', () =>
      planApi.savePlanDraft(path, content),
    );
  }

  async function handlePublishPlan(revisionId: string) {
    if (!workspace) {
      return false;
    }
    const path = workspace.status.rootPath;
    return runWorkspaceAction('正在发布计划', () =>
      planApi.publishPlan(path, revisionId, true),
    );
  }

  async function handlePreviewPlanPublish(
    revisionId: string,
  ): Promise<PlanPublishPreview | null> {
    if (!workspace) {
      return null;
    }
    try {
      return await planApi.previewPlanPublish(
        workspace.status.rootPath,
        revisionId,
      );
    } catch (previewError) {
      setError(normalizeCommandError(previewError));
      return null;
    }
  }

  async function handleCompleteHumanWork(
    workItemId: string,
    note: string,
    accepted: boolean,
  ) {
    if (!workspace) {
      return false;
    }
    const path = workspace.status.rootPath;
    return runWorkspaceAction(
      accepted ? '正在完成人工任务' : '正在标记人工阻塞',
      () => planApi.completeHumanWork(path, workItemId, note, accepted),
    );
  }

  async function handlePlanRefresh(): Promise<ProjectWorkspace | null> {
    if (!workspace) {
      return null;
    }
    try {
      const refreshed = await planApi.readPlanWorkspace(
        workspace.status.rootPath,
      );
      setWorkspace(refreshed);
      return refreshed;
    } catch (refreshError) {
      setError(normalizeCommandError(refreshError));
      return null;
    }
  }

  function handleNavigate(nextView: WorkspaceView) {
    if (nextView !== 'overview' && !workspace) {
      return;
    }
    setView(nextView);
  }

  const busy = busyLabel !== null;

  return (
    <div className="app-shell">
      <AppSidebar view={view} hasProject={workspace !== null} onNavigate={handleNavigate} />

      <main className="workspace">
        <div className="workspace__topline" aria-hidden="true">
          <span>LOCAL-FIRST PROJECT SYSTEM</span>
          <span>PHASE 08 / AI REVIEW GATE</span>
        </div>

        {error ? <ErrorBanner error={error} onDismiss={() => setError(null)} /> : null}

        <div className="workspace__content">
          {!workspace && busyLabel ? <LoadingView label={busyLabel} /> : null}

          {!workspace && !busyLabel ? (
            <WelcomeView
              onCreate={() => setIsCreateOpen(true)}
              onOpen={() => void handleOpenProject()}
            />
          ) : null}

          {workspace && view === 'overview' ? (
            <ProjectOverview
              workspace={workspace}
              busy={busy}
              onRefresh={() => void handleRefresh()}
              onEnterRequirements={() => setView('requirements')}
              onEnterPlan={() => setView('plan')}
            />
          ) : null}

          {workspace && view === 'requirements' ? (
            <RequirementsView
              sources={workspace.sources}
              busy={busy}
              busyLabel={busyLabel}
              onImport={() => void handleImport()}
              onGenerate={(config) => void handleGenerate(config)}
            />
          ) : null}

          {workspace && view === 'blueprint' ? (
            <BlueprintView
              workspace={workspace}
              busy={busy}
              busyLabel={busyLabel}
              onSave={handleSaveBlueprint}
              onPublish={handlePublishBlueprint}
              onNavigateRequirements={() => setView('requirements')}
            />
          ) : null}

          {workspace && view === 'inbox' ? (
            <InboxView
              inbox={workspace.inbox}
              facts={workspace.facts}
              humanTasks={workspace.plan.workItems.filter(
                (item) =>
                  (item.task.kind === 'human_external' ||
                    item.task.kind === 'approval_gate') &&
                  !['done', 'cancelled'].includes(item.workItem.status),
              )}
              busy={busy}
              busyLabel={busyLabel}
              onResolve={handleResolveInfoRequest}
              onCompleteHuman={handleCompleteHumanWork}
            />
          ) : null}

          {workspace && view === 'plan' ? (
            <PlanView
              workspace={workspace}
              busy={busy}
              busyLabel={busyLabel}
              onGenerate={handleGeneratePlan}
              onSave={handleSavePlan}
              onPreviewPublish={handlePreviewPlanPublish}
              onPublish={handlePublishPlan}
              onRefresh={handlePlanRefresh}
              onNavigateBlueprint={() => setView('blueprint')}
              onNavigateInbox={() => setView('inbox')}
              onNavigateReview={() => setView('review')}
            />
          ) : null}

          {workspace && view === 'integration' ? (
            <IntegrationView path={workspace.status.rootPath} />
          ) : null}

          {workspace && view === 'review' ? (
            <ReviewView
              path={workspace.status.rootPath}
              onPlanChanged={async () => {
                await handlePlanRefresh();
              }}
            />
          ) : null}

          {workspace && view === 'release' ? (
            <ReleaseCenterView
              path={workspace.status.rootPath}
              onEnterInbox={() => setView('inbox')}
            />
          ) : null}
        </div>
      </main>

      {isCreateOpen ? (
        <CreateProjectDialog
          submitting={busy}
          onCancel={() => setIsCreateOpen(false)}
          onPickDirectory={handlePickProjectHome}
          onCreate={(name, path) => void handleCreateProject(name, path)}
        />
      ) : null}
    </div>
  );
}
