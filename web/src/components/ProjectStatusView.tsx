import type { ProjectStatus } from '../api/types';

interface ProjectStatusViewProps {
  status: ProjectStatus;
  onRefresh: () => void;
}

export function ProjectStatusView({
  status,
  onRefresh,
}: ProjectStatusViewProps) {
  return (
    <section className="project-view" aria-labelledby="project-title">
      <header className="project-view__header">
        <div>
          <p className="eyebrow">PROJECT OVERVIEW / 已连接</p>
          <h1 id="project-title">{status.name}</h1>
          <p className="project-view__lead">
            项目上下文已载入，本地清单与状态数据库保持一致。
          </p>
        </div>
        <button className="button button--secondary" type="button" onClick={onRefresh}>
          刷新状态
        </button>
      </header>

      <dl className="status-card" aria-label="项目状态">
        <div className="status-card__item status-card__item--wide">
          <dt>项目 ID</dt>
          <dd className="mono">{status.projectId}</dd>
        </div>
        <div className="status-card__item status-card__item--wide">
          <dt>根路径</dt>
          <dd className="mono status-card__path">{status.rootPath}</dd>
        </div>
        <div className="status-card__item">
          <dt>创建时间</dt>
          <dd className="mono">
            <time dateTime={status.createdAt}>{status.createdAt}</time>
          </dd>
        </div>
        <div className="status-card__item status-card__item--metric">
          <dt>事件数</dt>
          <dd>
            <strong className="status-card__count">
              {`${status.eventCount} 个事件`}
            </strong>
          </dd>
        </div>
      </dl>

      <div className="project-view__footer">
        <span className="signal-line" aria-hidden="true" />
        <span>所有状态均来自本地 ColdStart 项目目录</span>
      </div>
    </section>
  );
}
