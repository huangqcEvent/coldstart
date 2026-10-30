import type { ProjectWorkspace } from '../api/types';

interface ProjectOverviewProps {
  workspace: ProjectWorkspace;
  busy: boolean;
  onRefresh: () => void;
  onEnterRequirements: () => void;
}

export function ProjectOverview({
  workspace,
  busy,
  onRefresh,
  onEnterRequirements,
}: ProjectOverviewProps) {
  const openInbox = workspace.inbox.filter(
    (request) => request.status === 'open',
  ).length;
  const draftLabel = workspace.draft
    ? `草稿 v${workspace.draft.revisionNumber}`
    : '暂无草稿';
  const publishedLabel = workspace.published
    ? `已发布 v${workspace.published.revisionNumber}`
    : '尚未发布';
  const nextStep =
    workspace.sources.length === 0
      ? '下一步：导入需求文档，让模型提出可追溯的结构化蓝图'
      : openInbox > 0
        ? '下一步：在人工 Inbox 处理阻塞问题，再继续推进蓝图'
        : workspace.draft
          ? '下一步：检查并发布当前蓝图草稿'
          : workspace.published
            ? '蓝图已发布；下一阶段将编译开发计划并交给外部 Agent 执行'
            : '下一步：在需求导入页生成蓝图草稿';

  return (
    <section className="project-view" aria-labelledby="project-title">
      <header className="project-view__header">
        <div>
          <p className="eyebrow">PROJECT OVERVIEW / 已连接</p>
          <h1 id="project-title">{workspace.status.name}</h1>
          <p className="project-view__lead">
            {workspace.status.rootPath} · {workspace.status.eventCount} 个事件
          </p>
        </div>
        <div className="view-actions">
          <button
            className="button button--secondary"
            type="button"
            onClick={onRefresh}
            disabled={busy}
          >
            刷新状态
          </button>
          <button
            className="button button--primary"
            type="button"
            onClick={onEnterRequirements}
          >
            进入需求导入
          </button>
        </div>
      </header>

      <div className="metrics-grid" aria-label="项目流程指标">
        <article className="metric-card">
          <span className="metric-card__index">01</span>
          <p>来源文档</p>
          <strong>{workspace.sources.length}</strong>
          <small>已固化到项目目录</small>
        </article>
        <article className="metric-card">
          <span className="metric-card__index">02</span>
          <p>蓝图草稿</p>
          <strong>{draftLabel}</strong>
          <small>{workspace.draft ? '可继续人工编辑' : '导入需求后生成'}</small>
        </article>
        <article className="metric-card">
          <span className="metric-card__index">03</span>
          <p>发布版本</p>
          <strong>{publishedLabel}</strong>
          <small>{workspace.published ? '当前有效蓝图' : '发布前需通过校验'}</small>
        </article>
        <article className="metric-card metric-card--attention">
          <span className="metric-card__index">04</span>
          <p>待处理 Inbox</p>
          <strong>{openInbox}</strong>
          <small>{openInbox > 0 ? '需要人工提供信息' : '当前无阻塞项'}</small>
        </article>
      </div>

      <dl className="status-card" aria-label="项目状态">
        <div className="status-card__item status-card__item--wide">
          <dt>项目 ID</dt>
          <dd className="mono">{workspace.status.projectId}</dd>
        </div>
        <div className="status-card__item">
          <dt>创建时间</dt>
          <dd className="mono">
            <time dateTime={workspace.status.createdAt}>
              {workspace.status.createdAt}
            </time>
          </dd>
        </div>
        <div className="status-card__item">
          <dt>项目事实</dt>
          <dd>{workspace.facts.length} 条已确认信息</dd>
        </div>
      </dl>

      <div className="project-view__footer">
        <span className="signal-line" aria-hidden="true" />
        <span>{nextStep}</span>
      </div>
    </section>
  );
}
