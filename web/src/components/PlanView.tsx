import { useEffect, useRef, useState } from 'react';

import type {
  PlanContent,
  PlanPublishPreview,
  PlanTaskDiff,
  ProjectWorkspace,
  TaskDependency,
  TaskSpec,
  WorkItemStatus,
  WorkItemView,
} from '../api/types';

interface PlanViewProps {
  workspace: ProjectWorkspace;
  busy: boolean;
  busyLabel: string | null;
  onGenerate: () => Promise<boolean>;
  onSave: (content: PlanContent) => Promise<boolean>;
  onPreviewPublish: (revisionId: string) => Promise<PlanPublishPreview | null>;
  onPublish: (revisionId: string) => Promise<boolean>;
  onRefresh: () => Promise<ProjectWorkspace | null>;
  onNavigateBlueprint: () => void;
  onNavigateInbox: () => void;
}

const statusLabels: Record<WorkItemStatus, string> = {
  draft: '草稿',
  ready: '可领取',
  claimed: '已领取',
  in_progress: '进行中',
  submitted: '已提交',
  done: '已完成',
  blocked: '已阻塞',
  failed: '失败',
  reopened: '重新打开',
  cancelled: '已取消',
};

const statusOrder: WorkItemStatus[] = [
  'claimed',
  'in_progress',
  'submitted',
  'ready',
  'blocked',
  'failed',
  'reopened',
  'draft',
  'done',
  'cancelled',
];

const kindLabels: Record<TaskSpec['kind'], string> = {
  agent_coding: 'Agent 编码',
  human_external: '人工外部',
  approval_gate: '人工审批',
  milestone: '里程碑',
};

const dependencyLabels: Record<TaskDependency['kind'], string> = {
  blocks: '阻塞',
  informs: '参考',
  conflicts_with: '冲突',
};

const verificationStatusLabels = {
  not_required: '无需验证',
  running: '验证中',
  passed: '已通过',
  failed: '未通过',
  interrupted: '已中断',
} as const;

const verificationFailureLabels = {
  non_zero_exit: '命令退出码非零',
  launch_failed: '命令启动失败',
  timeout: '命令超时',
  git_invariant: 'Git 不变量被破坏',
  infrastructure: '验证基础设施失败',
  interrupted: '租约过期或进程中断',
} as const;

function formatDateTime(value: string) {
  return new Date(value).toLocaleString('zh-CN', { hour12: false });
}

function clonePlanContent(content: PlanContent): PlanContent {
  return {
    warnings: [...content.warnings],
    tasks: content.tasks.map((task) => ({
      ...task,
      scope: [...task.scope],
      outOfScope: [...task.outOfScope],
      allowedPaths: [...task.allowedPaths],
      contextRefs: [...task.contextRefs],
      acceptanceCriteria: task.acceptanceCriteria.map((criterion) => ({
        ...criterion,
      })),
      evidenceRequirements: [...task.evidenceRequirements],
      resourceKeys: [...task.resourceKeys],
      dependencies: task.dependencies.map((dependency) => ({ ...dependency })),
    })),
  };
}

function linesToItems(value: string) {
  return value
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

function replaceBlockingDependencies(task: TaskSpec, value: string): TaskDependency[] {
  const retained = task.dependencies.filter(
    (dependency) => dependency.kind !== 'blocks',
  );
  const blocking = Array.from(
    new Set(
      value
        .split(/[\n,]/)
        .map((item) => item.trim())
        .filter((item) => item.length > 0),
    ),
  ).map<TaskDependency>((taskKey) => ({ taskKey, kind: 'blocks' }));
  return [...blocking, ...retained];
}

function PlanDraftEditor({
  content,
  onChange,
}: {
  content: PlanContent;
  onChange: (content: PlanContent) => void;
}) {
  function updateTask(index: number, update: Partial<TaskSpec>) {
    onChange({
      ...content,
      tasks: content.tasks.map((task, taskIndex) =>
        taskIndex === index ? { ...task, ...update } : task,
      ),
    });
  }

  return (
    <section className="plan-editor" aria-labelledby="plan-editor-title">
      <div className="panel__head">
        <div>
          <p className="panel__eyebrow">DRAFT BOUNDARY / 人工可编辑</p>
          <h2 id="plan-editor-title">计划草稿编辑</h2>
        </div>
        <span className="count-badge">{content.tasks.length} 个任务</span>
      </div>
      <p className="field__hint">
        只开放任务目标、范围、优先级、验收与阻塞依赖。保存时会重新校验 DAG、任务键和路径边界。
      </p>
      <div className="plan-editor__list">
        {content.tasks.map((task, index) => (
          <article className="plan-editor-card" key={task.taskKey}>
            <header>
              <div>
                <span>{kindLabels[task.kind]}</span>
                <code>{task.taskKey}</code>
              </div>
              <label className="plan-editor__flag">
                <input
                  type="checkbox"
                  checked={task.criticalPath}
                  onChange={(event) =>
                    updateTask(index, { criticalPath: event.target.checked })
                  }
                />
                <span>关键路径</span>
              </label>
            </header>
            <div className="plan-editor-card__grid">
              <label className="field">
                <span>标题</span>
                <input
                  value={task.title}
                  onChange={(event) =>
                    updateTask(index, { title: event.target.value })
                  }
                  aria-label={`标题：${task.taskKey}`}
                />
              </label>
              <label className="field plan-editor__priority">
                <span>优先级</span>
                <input
                  type="number"
                  value={task.priority}
                  onChange={(event) =>
                    updateTask(index, { priority: Number(event.target.value) })
                  }
                  aria-label={`优先级：${task.taskKey}`}
                />
              </label>
              <label className="field plan-editor-card__wide">
                <span>目标</span>
                <textarea
                  value={task.goal}
                  onChange={(event) =>
                    updateTask(index, { goal: event.target.value })
                  }
                  aria-label={`目标：${task.taskKey}`}
                  rows={3}
                />
              </label>
              <label className="field">
                <span>范围内（每行一项）</span>
                <textarea
                  value={task.scope.join('\n')}
                  onChange={(event) =>
                    updateTask(index, { scope: linesToItems(event.target.value) })
                  }
                  aria-label={`范围：${task.taskKey}`}
                  rows={3}
                />
              </label>
              <label className="field">
                <span>明确不做（每行一项）</span>
                <textarea
                  value={task.outOfScope.join('\n')}
                  onChange={(event) =>
                    updateTask(index, {
                      outOfScope: linesToItems(event.target.value),
                    })
                  }
                  aria-label={`不做：${task.taskKey}`}
                  rows={3}
                />
              </label>
              <div className="field plan-editor-card__wide">
                <span>验收标准</span>
                {task.acceptanceCriteria.map((criterion, criterionIndex) => (
                  <div className="plan-editor__criterion" key={criterion.id}>
                    <code>{criterion.id}</code>
                    <input
                      value={criterion.text}
                      onChange={(event) => {
                        const acceptanceCriteria = task.acceptanceCriteria.map(
                          (item, itemIndex) =>
                            itemIndex === criterionIndex
                              ? { ...item, text: event.target.value }
                              : item,
                        );
                        updateTask(index, { acceptanceCriteria });
                      }}
                      aria-label={`验收标准：${task.taskKey} / ${criterion.id}`}
                    />
                    <label className="plan-editor__flag">
                      <input
                        type="checkbox"
                        checked={criterion.requiredEvidence}
                        onChange={(event) => {
                          const acceptanceCriteria = task.acceptanceCriteria.map(
                            (item, itemIndex) =>
                              itemIndex === criterionIndex
                                ? {
                                    ...item,
                                    requiredEvidence: event.target.checked,
                                  }
                                : item,
                          );
                          updateTask(index, { acceptanceCriteria });
                        }}
                      />
                      <span>需要证据</span>
                    </label>
                  </div>
                ))}
              </div>
              <label className="field">
                <span>证据要求（每行一项）</span>
                <textarea
                  value={task.evidenceRequirements.join('\n')}
                  onChange={(event) =>
                    updateTask(index, {
                      evidenceRequirements: linesToItems(event.target.value),
                    })
                  }
                  aria-label={`证据要求：${task.taskKey}`}
                  rows={3}
                />
              </label>
              <label className="field">
                <span>阻塞依赖（任务键，每行一个）</span>
                <textarea
                  value={task.dependencies
                    .filter((dependency) => dependency.kind === 'blocks')
                    .map((dependency) => dependency.taskKey)
                    .join('\n')}
                  onChange={(event) =>
                    updateTask(index, {
                      dependencies: replaceBlockingDependencies(
                        task,
                        event.target.value,
                      ),
                    })
                  }
                  aria-label={`阻塞依赖：${task.taskKey}`}
                  rows={3}
                />
              </label>
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}

function PlanTaskCard({
  item,
  blockers,
  onNavigateInbox,
}: {
  item: WorkItemView;
  blockers: ProjectWorkspace['plan']['blockers'];
  onNavigateInbox: () => void;
}) {
  const taskBlockers = blockers.filter(
    (blocker) => blocker.workItemId === item.workItem.id,
  );
  const isHuman =
    item.task.kind === 'human_external' || item.task.kind === 'approval_gate';
  const openHumanTask =
    isHuman &&
    !['done', 'cancelled'].includes(item.workItem.status);
  const verification = item.latestReport?.verification ?? null;
  const failedCommand = verification?.commands.find(
    (command) => command.name === verification.failureCommand,
  );

  return (
    <article className={`plan-task plan-task--${item.workItem.status}`}>
      <header className="plan-task__head">
        <div>
          <span className={`plan-status plan-status--${item.workItem.status}`}>
            {statusLabels[item.workItem.status]}
          </span>
          <span className="plan-kind">{kindLabels[item.task.kind]}</span>
          {item.task.criticalPath ? <span className="plan-kind">关键路径</span> : null}
          {item.workItem.contractState === 'stale' ? (
            <span className="plan-kind plan-kind--warning">契约已过期</span>
          ) : null}
        </div>
        <span className="plan-task__priority">P{item.task.priority}</span>
      </header>
      <h3>{item.task.title}</h3>
      <p className="plan-task__goal">{item.task.goal}</p>
      <code className="plan-task__key">{item.task.taskKey}</code>

      <div className="plan-task__tags">
        <span>平台 {item.task.platformId ?? '通用'}</span>
        <span>模块 {item.task.moduleId ?? '通用'}</span>
        <span>仓库 {item.task.repositoryKey ?? '无'}</span>
        <span>预算 {item.task.contextBudgetTokens.toLocaleString()} tokens</span>
      </div>

      {item.task.dependencies.length > 0 ? (
        <div className="plan-task__section">
          <strong>依赖</strong>
          <div className="plan-dependencies">
            {item.task.dependencies.map((dependency) => (
              <span key={`${dependency.kind}:${dependency.taskKey}`}>
                {dependencyLabels[dependency.kind]} · {dependency.taskKey}
              </span>
            ))}
          </div>
        </div>
      ) : null}

      <div className="plan-task__section">
        <strong>验收标准</strong>
        <ul className="plan-acceptance">
          {item.task.acceptanceCriteria.map((criterion) => (
            <li key={criterion.id}>
              <code>{criterion.id}</code>
              <span>{criterion.text}</span>
              {criterion.requiredEvidence ? <em>需证据</em> : null}
            </li>
          ))}
        </ul>
      </div>

      {item.activeLease ? (
        <div className="plan-task__lease">
          <span>当前租约</span>
          <strong>{item.activeLease.agentProfile}</strong>
          <time dateTime={item.activeLease.expiresAt}>
            至 {item.activeLease.expiresAt}
          </time>
        </div>
      ) : null}

      {taskBlockers.length > 0 ? (
        <div className="plan-task__blockers">
          <strong>人工阻塞</strong>
          {taskBlockers.map((blocker) => (
            <p key={`${blocker.createdAt}:${blocker.reason}`}>{blocker.reason}</p>
          ))}
        </div>
      ) : null}

      {item.latestReport ? (
        <div className="plan-task__report">
          <span>
            最近报告 · {item.latestReport.accepted ? '已验收' : '未验收'}
          </span>
          <p>{item.latestReport.report.summary}</p>
          {verification ? (
            <div
              className={`plan-task__verification plan-task__verification--${verification.status}`}
            >
              <div className="plan-task__verification-head">
                <strong>
                  仓库验证 · {verificationStatusLabels[verification.status]}
                </strong>
                {verification.failureKind ? (
                  <span>{verificationFailureLabels[verification.failureKind]}</span>
                ) : null}
              </div>
              {verification.failureCommand ? (
                <p>
                  失败命令 <code>{verification.failureCommand}</code>
                  {failedCommand ? (
                    <>
                      {' '}· 退出码 {failedCommand.exitCode ?? '未启动'} · 耗时{' '}
                      {failedCommand.durationMs} ms
                    </>
                  ) : null}
                </p>
              ) : (
                <p>
                  {verification.commands.length} 条命令全部通过 · 总耗时{' '}
                  {verification.totalDurationMs ?? 0} ms
                </p>
              )}
              {failedCommand?.outputSummary ? (
                <details>
                  <summary>查看脱敏输出摘要</summary>
                  <pre>{failedCommand.outputSummary}</pre>
                </details>
              ) : null}
              <small>
                快照 {verification.snapshotHash.slice(0, 12)}… · 开始{' '}
                {formatDateTime(verification.startedAt)}
              </small>
            </div>
          ) : null}
          {item.latestReport.rejectionReasons.length > 0 ? (
            <ul>
              {item.latestReport.rejectionReasons.map((reason) => (
                <li key={reason}>{reason}</li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : null}

      {openHumanTask ? (
        <button
          className="button button--secondary plan-task__inbox"
          type="button"
          onClick={onNavigateInbox}
          aria-label={`转到人工 Inbox：${item.task.title}`}
        >
          转到人工 Inbox
        </button>
      ) : null}
    </article>
  );
}

function PlanDiffGroup({
  label,
  items,
}: {
  label: string;
  items: PlanTaskDiff[];
}) {
  if (items.length === 0) {
    return null;
  }
  return (
    <section className="plan-diff-group">
      <h3>
        {label} · {items.length}
      </h3>
      <div>
        {items.map((item) => (
          <article key={item.taskKey}>
            <span>{item.title}</span>
            <code>{item.taskKey}</code>
            {item.contractChanged ? <em>契约变化</em> : null}
          </article>
        ))}
      </div>
    </section>
  );
}

export function PlanView({
  workspace,
  busy,
  busyLabel,
  onGenerate,
  onSave,
  onPreviewPublish,
  onPublish,
  onRefresh,
  onNavigateBlueprint,
  onNavigateInbox,
}: PlanViewProps) {
  const plan = workspace.plan;
  const activeRevision = plan.draft ?? plan.published;
  const [draftContent, setDraftContent] = useState<PlanContent | null>(() =>
    plan.draft ? clonePlanContent(plan.draft.content) : null,
  );
  const [isEditing, setIsEditing] = useState(false);
  const [isDirty, setIsDirty] = useState(false);
  const [copied, setCopied] = useState(false);
  const [publishPreview, setPublishPreview] =
    useState<PlanPublishPreview | null>(null);
  const [previewingPublish, setPreviewingPublish] = useState(false);
  const refreshing = useRef(false);
  const publishButtonRef = useRef<HTMLButtonElement>(null);
  const publishDialogTitleRef = useRef<HTMLHeadingElement>(null);

  useEffect(() => {
    setDraftContent(plan.draft ? clonePlanContent(plan.draft.content) : null);
    setIsEditing(false);
    setIsDirty(false);
  }, [plan.draft?.id]);

  useEffect(() => {
    let disposed = false;
    const refresh = async () => {
      if (disposed || refreshing.current) {
        return;
      }
      refreshing.current = true;
      try {
        await onRefresh();
      } finally {
        refreshing.current = false;
      }
    };
    const timer = window.setInterval(() => void refresh(), 3_000);
    const handleFocus = () => void refresh();
    window.addEventListener('focus', handleFocus);
    return () => {
      disposed = true;
      window.clearInterval(timer);
      window.removeEventListener('focus', handleFocus);
    };
  }, [onRefresh]);

  useEffect(() => {
    if (publishPreview) {
      publishDialogTitleRef.current?.focus();
    }
  }, [publishPreview]);

  const doneCount = plan.workItems.filter(
    (item) => item.workItem.status === 'done',
  ).length;
  const readyCount = plan.workItems.filter(
    (item) => item.workItem.status === 'ready',
  ).length;
  const activeLeaseCount = plan.workItems.filter(
    (item) => item.activeLease?.status === 'active',
  ).length;
  const expectedTaskCount = activeRevision?.content.tasks.length ?? 0;
  const publishBusy = busyLabel === '正在发布计划';
  const saveBusy = busyLabel === '正在保存计划草稿';
  const generateBusy = busyLabel === '正在编译开发计划';

  async function handleGenerate() {
    const generated = await onGenerate();
    if (generated) {
      setIsEditing(false);
      setIsDirty(false);
    }
  }

  async function handleSave() {
    if (!draftContent) {
      return;
    }
    const saved = await onSave(draftContent);
    if (saved) {
      setIsEditing(false);
      setIsDirty(false);
    }
  }

  async function handlePreviewPublish() {
    if (!plan.draft) {
      return;
    }
    setPreviewingPublish(true);
    try {
      const preview = await onPreviewPublish(plan.draft.id);
      if (preview) {
        setPublishPreview(preview);
      }
    } finally {
      setPreviewingPublish(false);
    }
  }

  async function handleConfirmPublish() {
    if (!plan.draft || !publishPreview?.publishable) {
      return;
    }
    const published = await onPublish(plan.draft.id);
    if (published) {
      setPublishPreview(null);
    }
  }

  function closePublishPreview() {
    setPublishPreview(null);
    window.setTimeout(() => publishButtonRef.current?.focus(), 0);
  }

  async function handleCopyMcpConfig() {
    try {
      await navigator.clipboard.writeText(plan.mcp.configJson);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1_500);
    } catch {
      setCopied(false);
    }
  }

  return (
    <section className="content-view plan-view" aria-labelledby="plan-title">
      <header className="content-view__header">
        <div>
          <p className="eyebrow">PLAN ENGINE / CODEX PULL PROTOCOL</p>
          <h1 id="plan-title">计划看板</h1>
          <p className="content-view__lead">
            从已发布蓝图生成不可变任务 DAG。Codex 通过 MCP 主动领取，只有租约内的结构化报告能完成编码任务。
          </p>
        </div>
        <div className="revision-summary">
          <span className="status-dot">
            {plan.draft
              ? `计划草稿 v${plan.draft.revisionNumber}`
              : plan.published
                ? `已发布 v${plan.published.revisionNumber}`
                : '尚无计划'}
          </span>
          {activeRevision ? <code>{activeRevision.templateVersion}</code> : null}
        </div>
      </header>

      <div className="plan-metrics" aria-label="计划指标">
        <article>
          <span>任务总数</span>
          <strong>{plan.workItems.length || expectedTaskCount}</strong>
        </article>
        <article>
          <span>可领取任务</span>
          <strong>{readyCount}</strong>
        </article>
        <article>
          <span>进行中</span>
          <strong>{activeLeaseCount}</strong>
        </article>
        <article>
          <span>已完成</span>
          <strong>
            {doneCount}/{plan.workItems.length || expectedTaskCount}
          </strong>
        </article>
      </div>

      <section className="plan-controls panel">
        <div>
          <p className="panel__eyebrow">COMPILE / EDIT / PUBLISH</p>
          <h2>开发计划</h2>
          <p>
            计划由 `plan-v1` 模板确定性生成，不再次调用模型。发布后任务状态、租约和报告都将持久化到本地。
          </p>
        </div>
        <div className="view-actions">
          {workspace.published ? (
            <button
              className="button button--secondary"
              type="button"
              onClick={() => void handleGenerate()}
              disabled={busy}
            >
              {generateBusy ? '编译中…' : '编译开发计划'}
            </button>
          ) : (
            <button
              className="button button--secondary"
              type="button"
              onClick={onNavigateBlueprint}
            >
              先发布蓝图
            </button>
          )}
          {plan.draft && draftContent ? (
            <button
              className="button button--secondary"
              type="button"
              onClick={() => setIsEditing((value) => !value)}
              disabled={busy}
            >
              {isEditing ? '收起编辑器' : '编辑计划草稿'}
            </button>
          ) : null}
          {plan.draft ? (
            <button
              className="button button--primary"
              type="button"
              onClick={() => void handlePreviewPublish()}
              disabled={busy || isDirty || previewingPublish}
              ref={publishButtonRef}
            >
              {previewingPublish ? '检查中…' : '发布计划'}
            </button>
          ) : null}
        </div>
        {isDirty ? (
          <p className="field__hint field__hint--warning">
            当前计划有未保存修改，请先保存草稿再发布。
          </p>
        ) : null}
      </section>

      {publishPreview ? (
        <div className="plan-publish-backdrop">
          <section
            className="plan-publish-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="plan-publish-dialog-title"
          >
            <div className="panel__head">
              <div>
                <p className="panel__eyebrow">PUBLISH DIFF / HUMAN CONFIRM</p>
                <h2
                  id="plan-publish-dialog-title"
                  ref={publishDialogTitleRef}
                  tabIndex={-1}
                >
                  {publishPreview.baseRevisionNumber === null
                    ? `确认发布计划 v${publishPreview.candidateRevisionNumber}`
                    : `确认替换计划 v${publishPreview.candidateRevisionNumber}`}
                </h2>
              </div>
              <span className="count-badge">
                {publishPreview.baseRevisionNumber === null
                  ? '首个活动计划'
                  : `替换 v${publishPreview.baseRevisionNumber}`}
              </span>
            </div>
            <p>
              这是人工确认步骤。确认后旧计划会标记为 superseded，历史契约、报告和事件仍保留用于审计。
            </p>

            <div className="plan-publish-metrics" aria-label="计划差异摘要">
              <article>
                <span>新增</span>
                <strong>{publishPreview.added.length}</strong>
              </article>
              <article>
                <span>契约变化</span>
                <strong>{publishPreview.changed.length}</strong>
              </article>
              <article>
                <span>保持不变</span>
                <strong>{publishPreview.unchanged.length}</strong>
              </article>
              <article>
                <span>删除</span>
                <strong>{publishPreview.removed.length}</strong>
              </article>
              <article>
                <span>继承完成</span>
                <strong>{publishPreview.carriedCompleted}</strong>
              </article>
              <article>
                <span>重新打开</span>
                <strong>{publishPreview.reopened}</strong>
              </article>
              <article>
                <span>取消</span>
                <strong>{publishPreview.cancelled}</strong>
              </article>
              <article>
                <span>活动租约</span>
                <strong>{publishPreview.activeLeaseCount}</strong>
              </article>
            </div>

            {publishPreview.blockers.length > 0 ? (
              <div className="plan-publish-blockers" role="alert">
                <strong>当前不能发布</strong>
                <ul>
                  {publishPreview.blockers.map((blocker) => (
                    <li key={blocker}>{blocker}</li>
                  ))}
                </ul>
              </div>
            ) : null}

            <div className="plan-diff-groups">
              <PlanDiffGroup label="新增" items={publishPreview.added} />
              <PlanDiffGroup label="契约变化" items={publishPreview.changed} />
              <PlanDiffGroup label="保持不变" items={publishPreview.unchanged} />
              <PlanDiffGroup label="删除" items={publishPreview.removed} />
            </div>

            <div className="plan-publish-actions">
              <button
                className="button button--secondary"
                type="button"
                onClick={closePublishPreview}
                disabled={busy}
              >
                取消发布
              </button>
              <button
                className="button button--primary"
                type="button"
                onClick={() => void handleConfirmPublish()}
                disabled={busy || !publishPreview.publishable}
              >
                {publishBusy
                  ? '发布中…'
                  : publishPreview.baseRevisionId
                    ? '确认替换活动计划'
                    : '确认发布首个计划'}
              </button>
            </div>
          </section>
        </div>
      ) : null}

      {isEditing && draftContent ? (
        <>
          <PlanDraftEditor
            content={draftContent}
            onChange={(content) => {
              setDraftContent(content);
              setIsDirty(true);
            }}
          />
          <div className="editor-actions plan-editor-actions">
            <div>
              <p className="panel__eyebrow">VALIDATE / SAVE</p>
              <p>保存会创建新的计划草稿版本，并重新执行 DAG 与契约校验。</p>
            </div>
            <button
              className="button button--primary"
              type="button"
              onClick={() => void handleSave()}
              disabled={busy}
            >
              {saveBusy ? '保存中…' : '保存计划草稿'}
            </button>
          </div>
        </>
      ) : null}

      {!activeRevision ? (
        <section className="panel plan-empty">
          <div className="empty-state">
            <p className="panel__eyebrow">NO PLAN</p>
            <strong>还没有开发计划</strong>
            <p>发布蓝图后，编译一份包含平台基础、模块切片、能力准备和发布门禁的任务 DAG。</p>
          </div>
        </section>
      ) : null}

      {activeRevision && plan.workItems.length === 0 ? (
        <section className="panel plan-preview">
          <div className="panel__head">
            <div>
              <p className="panel__eyebrow">DRAFT PREVIEW</p>
              <h2>发布前预览</h2>
            </div>
            <span className="count-badge">{activeRevision.content.tasks.length} 个任务</span>
          </div>
          <div className="plan-preview__list">
            {activeRevision.content.tasks.map((task) => (
              <div key={task.taskKey}>
                <span>{kindLabels[task.kind]}</span>
                <strong>{task.title}</strong>
                <code>{task.taskKey}</code>
              </div>
            ))}
          </div>
          {activeRevision.content.warnings.length > 0 ? (
            <ul className="plan-warnings">
              {activeRevision.content.warnings.map((warning) => (
                <li key={warning}>{warning}</li>
              ))}
            </ul>
          ) : null}
        </section>
      ) : null}

      {plan.workItems.length > 0 ? (
        <div className="plan-groups">
          {statusOrder.map((status) => {
            const items = plan.workItems.filter(
              (item) => item.workItem.status === status,
            );
            if (items.length === 0) {
              return null;
            }
            return (
              <section className="plan-group" key={status} aria-labelledby={`plan-${status}`}>
                <header>
                  <h2 id={`plan-${status}`}>
                    {statusLabels[status]} · {items.length}
                  </h2>
                  <span>{status}</span>
                </header>
                <div className="plan-task-grid">
                  {items.map((item) => (
                    <PlanTaskCard
                      item={item}
                      blockers={plan.blockers}
                      onNavigateInbox={onNavigateInbox}
                      key={item.workItem.id}
                    />
                  ))}
                </div>
              </section>
            );
          })}
        </div>
      ) : null}

      <section className="panel mcp-panel" aria-labelledby="mcp-title">
        <div className="panel__head">
          <div>
            <p className="panel__eyebrow">MCP / CODEX PULL</p>
            <h2 id="mcp-title">连接 Codex</h2>
          </div>
          <button
            className="button button--secondary"
            type="button"
            onClick={() => void handleCopyMcpConfig()}
          >
            {copied ? '已复制' : '复制 MCP 配置'}
          </button>
        </div>
        <p>
          ColdStart 不启动 Codex。把以下配置加入支持 MCP 的 Agent，让它主动领取任务；租约令牌不会写入 UI。
        </p>
        <pre>{plan.mcp.configJson}</pre>
        <div className="mcp-panel__rules">
          <span>默认租约 30 分钟</span>
          <span>建议每 5 分钟心跳</span>
          <span>默认上下文 12,000 tokens</span>
          <span>一次只领取一个任务</span>
        </div>
      </section>
    </section>
  );
}
