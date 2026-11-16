/**
 * 桌面主界面与工作区状态协调器。
 *
 * 负责启动恢复、项目切换、工作区刷新、导航和全局错误展示；应用级最近项目只
 * 保存路径与打开时间，业务状态由 Tauri 命令返回，前端不自行推断任务状态。
 */
import { useEffect, useState } from 'react';

import * as planApi from './api/planApi';
import * as projectApi from './api/projectApi';
import * as requirementsApi from './api/requirementsApi';
import * as issueApi from './api/issueApi';
import * as investigationApi from './api/investigationApi';
import type {
  BlueprintContent,
  CommandError,
  ModelConfig,
  PlanContent,
  PlanPublishPreview,
  ProjectHistoryView,
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

const emptyProjectHistory: ProjectHistoryView = {
  schemaVersion: 1,
  lastProjectPath: null,
  projects: [],
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
  const [projectHistory, setProjectHistory] =
    useState<ProjectHistoryView>(emptyProjectHistory);
  const [restoreNotice, setRestoreNotice] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function restoreLastProject() {
      try {
        const history = await projectApi.readProjectHistory();
        if (cancelled) {
          return;
        }
        setProjectHistory(history);
        if (!history.lastProjectPath) {
          return;
        }

        const lastProject = history.projects.find(
          (project) => project.rootPath === history.lastProjectPath,
        );
        if (!lastProject?.available) {
          setRestoreNotice('上次项目当前不可用');
          return;
        }

        setBusyLabel('正在恢复上次项目');
        setError(null);
        try {
          const [status, loadedWorkspace] = await Promise.all([
            projectApi.readProjectStatus(lastProject.rootPath),
            projectApi.readProjectWorkspace(lastProject.rootPath),
          ]);
          if (cancelled) {
            return;
          }
          setWorkspace({ ...loadedWorkspace, status });
          setView('overview');
          setRestoreNotice(null);
        } catch {
          if (!cancelled) {
            setRestoreNotice('上次项目当前不可用');
            setProjectHistory((current) => ({
              ...current,
              projects: current.projects.map((project) =>
                project.rootPath === lastProject.rootPath
                  ? { ...project, available: false }
                  : project,
              ),
            }));
          }
        } finally {
          if (!cancelled) {
            setBusyLabel(null);
          }
        }
      } catch (historyError) {
        if (!cancelled) {
          setError(normalizeCommandError(historyError));
        }
      }
    }

    void restoreLastProject();
    return () => {
      cancelled = true;
    };
  }, []);

  async function rememberProject(path: string) {
    try {
      const history = await projectApi.rememberRecentProject(path);
      setProjectHistory(history);
      setRestoreNotice(null);
    } catch (historyError) {
      setError(normalizeCommandError(historyError));
    }
  }

  async function loadProject(path: string, label: string, remember = true) {
    setBusyLabel(label);
    setError(null);
    try {
      const [status, loadedWorkspace] = await Promise.all([
        projectApi.readProjectStatus(path),
        projectApi.readProjectWorkspace(path),
      ]);
      setWorkspace({ ...loadedWorkspace, status });
      setView('overview');
      setRestoreNotice(null);
      if (remember) {
        await rememberProject(path);
      }
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
      setRestoreNotice(null);
      await rememberProject(path);
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

  async function handleSelectRecentProject(path: string) {
    if (busyLabel || workspace?.status.rootPath === path) {
      return;
    }
    await loadProject(path, '正在切换项目');
  }

  async function handleForgetRecentProject(path: string) {
    if (busyLabel) {
      return;
    }
    const removingCurrent = workspace?.status.rootPath === path;
    setBusyLabel('正在更新项目历史');
    setError(null);
    try {
      const history = await projectApi.forgetRecentProject(path);
      setProjectHistory(history);
      setRestoreNotice(null);
      if (removingCurrent) {
        setWorkspace(null);
        setView('overview');
      }
    } catch (historyError) {
      setError(normalizeCommandError(historyError));
    } finally {
      setBusyLabel(null);
    }
  }

  async function handleClearProjectHistory() {
    if (busyLabel) {
      return;
    }
    setBusyLabel('正在清除访问历史');
    setError(null);
    try {
      const history = await projectApi.clearProjectHistory();
      setProjectHistory(history);
      setRestoreNotice(null);
      setWorkspace(null);
      setView('overview');
    } catch (historyError) {
      setError(normalizeCommandError(historyError));
    } finally {
      setBusyLabel(null);
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

  async function handleGenerate(config: ModelConfig, artifactIds: string[]) {
    if (!workspace) {
      return;
    }
    const path = workspace.status.rootPath;
    await runWorkspaceAction('正在生成蓝图草稿', () =>
      requirementsApi.generateBlueprint(path, config, artifactIds),
    );
  }

  async function handleCreateCodeInvestigation(
    repositoryKey: string,
    baseRef: string,
    question: string,
  ) {
    if (!workspace) {
      return false;
    }
    const path = workspace.status.rootPath;
    return runWorkspaceAction('正在创建历史代码调查', () =>
      investigationApi.createCodeInvestigation(
        path,
        repositoryKey,
        baseRef.trim() || null,
        question,
      ),
    );
  }

  async function handleConfirmCodeInvestigation(investigationId: string) {
    if (!workspace) {
      return false;
    }
    const path = workspace.status.rootPath;
    return runWorkspaceAction('正在确认历史代码调查', () =>
      investigationApi.confirmCodeInvestigation(path, investigationId),
    );
  }

  async function handleRejectCodeInvestigation(
    investigationId: string,
    reason: string,
  ) {
    if (!workspace) {
      return false;
    }
    const path = workspace.status.rootPath;
    return runWorkspaceAction('正在驳回历史代码调查', () =>
      investigationApi.rejectCodeInvestigation(path, investigationId, reason),
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

  async function handleDismissInfoRequest(requestId: string, reason: string) {
    if (!workspace) {
      return false;
    }
    const path = workspace.status.rootPath;
    return runWorkspaceAction('正在关闭 Inbox 问题', () =>
      requirementsApi.dismissInfoRequest(path, requestId, reason),
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

  async function handleMarkWorkIntegrated(
    workItemId: string,
    reference: string,
  ) {
    if (!workspace) {
      return false;
    }
    const path = workspace.status.rootPath;
    return runWorkspaceAction('正在确认合入基线', () =>
      planApi.markWorkIntegrated(path, workItemId, reference),
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
      <AppSidebar
        view={view}
        hasProject={workspace !== null}
        project={workspace?.status ?? null}
        projectHistory={projectHistory.projects}
        busy={busy}
        onNavigate={handleNavigate}
        onSelectProject={(path) => void handleSelectRecentProject(path)}
        onOpenProject={() => void handleOpenProject()}
        onCreateProject={() => setIsCreateOpen(true)}
        onForgetProject={(path) => void handleForgetRecentProject(path)}
        onClearProjectHistory={() => void handleClearProjectHistory()}
      />

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
              recentProjects={projectHistory.projects}
              lastProjectPath={projectHistory.lastProjectPath}
              restoreNotice={restoreNotice}
              onCreate={() => setIsCreateOpen(true)}
              onOpen={() => void handleOpenProject()}
              onOpenRecent={(path) => void handleSelectRecentProject(path)}
              onForgetRecent={(path) => void handleForgetRecentProject(path)}
              onClearHistory={() => void handleClearProjectHistory()}
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
              sourceAnchors={workspace.sourceAnchors}
              sourceImages={workspace.sourceImages}
              codeInvestigations={workspace.codeInvestigations}
              busy={busy}
              busyLabel={busyLabel}
              onImport={() => void handleImport()}
              onGenerate={(config, artifactIds) =>
                void handleGenerate(config, artifactIds)
              }
              onImportIssue={handleImportGitHubIssue}
              onCreateCodeInvestigation={handleCreateCodeInvestigation}
              onConfirmCodeInvestigation={handleConfirmCodeInvestigation}
              onRejectCodeInvestigation={handleRejectCodeInvestigation}
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
              onDismiss={handleDismissInfoRequest}
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
              onMarkIntegrated={handleMarkWorkIntegrated}
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
