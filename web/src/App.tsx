/**
 * 桌面主界面与工作区状态协调器。
 *
 * 负责项目打开/创建、工作区刷新、导航和全局错误展示；业务状态由 Tauri 命令返回，
 * 前端不自行推断任务或外部服务状态。
 */
import { useState } from 'react';

import * as planApi from './api/planApi';
import * as projectApi from './api/projectApi';
import * as requirementsApi from './api/requirementsApi';
import * as issueApi from './api/issueApi';
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
import { ProjectMemoryView } from './components/ProjectMemoryView';
import { ReleaseCenterView } from './components/ReleaseCenterView';
import { RequirementsView } from './components/RequirementsView';
import { ReviewView } from './components/ReviewView';
import { ThemeToggle } from './components/ThemeToggle';
import { WelcomeView } from './components/WelcomeView';

const viewLabels: Record<WorkspaceView, string> = {
  overview: '项目总览',
  requirements: '需求导入',
  blueprint: '蓝图编排',
  plan: '计划看板',
  inbox: '人工 Inbox',
  integration: '仓库与能力',
  review: 'AI 评审',
  release: '发布中心',
  memory: '项目记忆',
};

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
      <p className="eyebrow">正在同步本地工作区</p>
      <h1>{label}</h1>
      <p>正在读取项目清单、来源文档与本地蓝图状态，请稍候。</p>
    </section>
  );
}

/** 协调项目加载、创建、刷新、导航和全局错误状态。 */
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

  async function handleImportGitHubIssue(
    repositoryKey: string,
    issueNumber: number,
  ) {
    if (!workspace) {
      return false;
    }
    const path = workspace.status.rootPath;
    return runWorkspaceAction('正在导入 GitHub Issue', () =>
      issueApi.importGitHubIssue(path, repositoryKey, issueNumber),
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
        <div className="workspace__topline">
          <div className="workspace__route">
            <span>ColdStart</span>
            <span className="workspace__route-separator" aria-hidden="true">/</span>
            <strong>{workspace ? viewLabels[view] : '欢迎'}</strong>
          </div>
          <div className="workspace__actions">
            <ThemeToggle />
            <span className="workspace__local-status">
              <span className="workspace__status-dot" aria-hidden="true" />
              本地数据
            </span>
          </div>
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
              onEnterIntegration={() => setView('integration')}
            />
          ) : null}

          {workspace && view === 'requirements' ? (
            <RequirementsView
              path={workspace.status.rootPath}
              sources={workspace.sources}
              busy={busy}
              busyLabel={busyLabel}
              onImport={() => void handleImport()}
              onGenerate={(config) => void handleGenerate(config)}
              onImportIssue={handleImportGitHubIssue}
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

          {workspace && view === 'memory' ? (
            <ProjectMemoryView path={workspace.status.rootPath} />
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
