import { useEffect, useId, useMemo, useState } from 'react';

import * as memoryApi from '../api/memoryApi';
import type {
  CommandError,
  MemoryPhase,
  ProjectMemoryItem,
  ProjectMemoryPage,
} from '../api/types';
import { ErrorBanner } from './ErrorBanner';

const MEMORY_PAGE_LIMIT = 100;

const phaseLabels: Record<MemoryPhase, string> = {
  requirements_blueprint: '需求与蓝图',
  plan_execution: '计划与执行',
  verification_review: '验证与评审',
  repository_capability: '仓库与能力',
  system_configuration: '系统与配置',
};

const eventTypeLabels: Record<string, string> = {
  project_initialized: '项目初始化',
  artifact_imported: '文档导入',
  blueprint_draft_created: '蓝图草稿',
  blueprint_published: '蓝图发布',
  info_request_created: '人工请求',
  info_request_resolved: '人工确认',
  plan_draft_created: '计划草稿',
  plan_published: '计划发布',
  plan_superseded: '计划替换',
  work_claimed: '任务领取',
  work_started: '开始执行',
  work_heartbeat: '任务心跳',
  work_blocked: '任务阻塞',
  work_report_submitted: '报告提交',
  work_completed: '任务完成',
  verification_started: '验证开始',
  verification_passed: '验证通过',
  verification_failed: '验证失败',
  verification_interrupted: '验证中断',
  review_started: '评审开始',
  review_passed: '评审通过',
  review_blocked: '评审阻断',
  review_failed: '评审失败',
  review_interrupted: '评审中断',
  review_waived: '人工豁免',
  work_released: '任务释放',
  lease_expired: '租约过期',
  human_work_completed: '人工任务',
  repository_saved: '仓库配置',
  repository_verification_commands_updated: '验证命令',
  credential_saved: '凭据保存',
  credential_deleted: '凭据删除',
  capability_updated: '能力更新',
  git_sync_recorded: 'Git 同步',
  pull_request_opened: 'PR 创建',
  pull_request_state_changed: 'PR 状态',
  project_setting_updated: '项目设置',
};

function eventTypeLabel(eventType: string): string {
  return eventTypeLabels[eventType] ?? eventType;
}

const phaseOptions: Array<{ phase: MemoryPhase | null; label: string }> = [
  { phase: null, label: '全部' },
  { phase: 'requirements_blueprint', label: phaseLabels.requirements_blueprint },
  { phase: 'plan_execution', label: phaseLabels.plan_execution },
  { phase: 'verification_review', label: phaseLabels.verification_review },
  { phase: 'repository_capability', label: phaseLabels.repository_capability },
  { phase: 'system_configuration', label: phaseLabels.system_configuration },
];

function normalizeMemoryError(error: unknown): CommandError {
  if (typeof error === 'object' && error !== null) {
    const candidate = error as Record<string, unknown>;
    if (typeof candidate.code === 'string' && typeof candidate.message === 'string') {
      return { code: candidate.code, message: candidate.message };
    }
  }
  return { code: 'unknown', message: String(error) };
}

function formatTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return new Intl.DateTimeFormat('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  }).format(date);
}

function formatActivity(value: string | undefined): string {
  if (!value) {
    return '暂无活动';
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return new Intl.DateTimeFormat('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  }).format(date);
}

function matchesSearch(item: ProjectMemoryItem, query: string): boolean {
  if (!query) {
    return true;
  }
  const searchable = [
    item.title,
    item.summary,
    item.eventType,
    eventTypeLabel(item.eventType),
    item.reference?.label ?? '',
    item.reference?.id ?? '',
  ]
    .join('\n')
    .toLowerCase();
  return searchable.includes(query.toLowerCase());
}

function phaseCount(page: ProjectMemoryPage, phase: MemoryPhase | null): number {
  if (phase === null) {
    return page.visibleCount;
  }
  return page.phaseCounts.find((entry) => entry.phase === phase)?.count ?? 0;
}

interface ProjectMemoryViewProps {
  path: string;
}

export function ProjectMemoryView({ path }: ProjectMemoryViewProps) {
  const searchId = useId();
  const heartbeatId = useId();
  const [page, setPage] = useState<ProjectMemoryPage | null>(null);
  const [selectedPhase, setSelectedPhase] = useState<MemoryPhase | null>(null);
  const [query, setQuery] = useState('');
  const [includeHeartbeats, setIncludeHeartbeats] = useState(false);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [reloadToken, setReloadToken] = useState(0);
  const [error, setError] = useState<CommandError | null>(null);

  useEffect(() => {
    let active = true;
    setLoading(true);
    setError(null);
    setPage(null);
    memoryApi
      .readProjectMemory(path, {
        beforeCursor: null,
        limit: MEMORY_PAGE_LIMIT,
        includeHeartbeats,
      })
      .then((loaded) => {
        if (active) {
          setPage(loaded);
        }
      })
      .catch((loadError: unknown) => {
        if (active) {
          setError(normalizeMemoryError(loadError));
        }
      })
      .finally(() => {
        if (active) {
          setLoading(false);
        }
      });

    return () => {
      active = false;
    };
  }, [includeHeartbeats, path, reloadToken]);

  async function loadMore() {
    if (!page?.nextCursor || loadingMore) {
      return;
    }
    setLoadingMore(true);
    setError(null);
    try {
      const nextPage = await memoryApi.readProjectMemory(path, {
        beforeCursor: page.nextCursor,
        limit: MEMORY_PAGE_LIMIT,
        includeHeartbeats,
      });
      setPage((current) => {
        if (!current) {
          return nextPage;
        }
        const existing = new Set(current.items.map((item) => item.eventId));
        return {
          ...nextPage,
          items: [
            ...current.items,
            ...nextPage.items.filter((item) => !existing.has(item.eventId)),
          ],
        };
      });
    } catch (loadError) {
      setError(normalizeMemoryError(loadError));
    } finally {
      setLoadingMore(false);
    }
  }

  const filteredItems = useMemo(() => {
    const normalizedQuery = query.trim();
    return (page?.items ?? []).filter(
      (item) =>
        (selectedPhase === null || item.phase === selectedPhase) &&
        matchesSearch(item, normalizedQuery),
    );
  }, [page, query, selectedPhase]);

  const hasFilters = selectedPhase !== null || query.trim().length > 0;
  const noProjectEvents = page !== null && page.totalCount === 0;
  const noVisibleEvents =
    page !== null && page.totalCount > 0 && page.items.length === 0 && !hasFilters;
  const recentActivity = page?.items[0]?.occurredAt;

  return (
    <section className="content-view memory-view" aria-labelledby="memory-view-title">
      <header className="content-view__header">
        <div>
          <p className="eyebrow">项目记忆与变更审计</p>
          <h1 id="memory-view-title">项目记忆</h1>
          <p className="content-view__lead">
            从本地事件日志还原项目演进。这里只展示经过安全投影的变更摘要，不修改、不删除，也不回放历史。
          </p>
        </div>
        <div className="view-actions memory-view__actions">
          <label className="memory-heartbeat-toggle" htmlFor={heartbeatId}>
            <input
              id={heartbeatId}
              type="checkbox"
              checked={includeHeartbeats}
              onChange={(event) => setIncludeHeartbeats(event.target.checked)}
              disabled={loading || loadingMore}
            />
            <span>显示系统心跳</span>
          </label>
          <button
            className="button button--secondary"
            type="button"
            onClick={() => setReloadToken((value) => value + 1)}
            disabled={loading || loadingMore}
          >
            刷新记忆
          </button>
        </div>
      </header>

      <div className="readonly-notice memory-readonly">
        <div>
          <strong>只读审计</strong>
          <p>事件来自项目 SQLite 日志，按发生顺序固化；界面不会暴露租约、密钥或原始 payload。</p>
        </div>
        <code>{path}</code>
      </div>

      {error ? <ErrorBanner error={error} onDismiss={() => setError(null)} /> : null}

      {loading ? (
        <section className="panel loading-view" role="status">
          <p className="eyebrow">正在读取本地事件</p>
          <h2>正在读取项目记忆</h2>
          <p>从本地事件日志还原最近的项目变更。</p>
        </section>
      ) : null}

      {!loading && page ? (
        <>
          <div className="metrics-grid memory-metrics" aria-label="项目记忆摘要">
            <article className="metric-card">
              <span className="metric-card__index">01</span>
              <p>全部事件</p>
              <strong>{page.totalCount}</strong>
              <small>包含系统心跳与业务变更</small>
            </article>
            <article className="metric-card">
              <span className="metric-card__index">02</span>
              <p>有效变更</p>
              <strong>{page.visibleCount}</strong>
              <small>不包含高频任务心跳</small>
            </article>
            <article className="metric-card">
              <span className="metric-card__index">03</span>
              <p>系统心跳</p>
              <strong>{page.heartbeatCount}</strong>
              <small>{includeHeartbeats ? '当前已显示' : '默认隐藏'}</small>
            </article>
            <article className="metric-card metric-card--attention">
              <span className="metric-card__index">04</span>
              <p>最近活动</p>
              <strong className="memory-metric-time">
                {formatActivity(recentActivity)}
              </strong>
              <small>{recentActivity ? '最近一条可见事件' : '等待第一条项目事件'}</small>
            </article>
          </div>

          <section className="panel memory-timeline-panel" aria-labelledby="memory-timeline-title">
            <div className="panel__head memory-toolbar">
              <div>
                <p className="panel__eyebrow">事件流 · 最新在前</p>
                <h2 id="memory-timeline-title">项目事件时间线</h2>
              </div>
              <span className="count-badge">{filteredItems.length}</span>
            </div>

            <div className="memory-controls">
              <div className="memory-phase-filters" role="group" aria-label="阶段筛选">
                {phaseOptions.map((option) => (
                  <button
                    className={`memory-phase-filter${
                      selectedPhase === option.phase ? ' memory-phase-filter--active' : ''
                    }`}
                    type="button"
                    aria-pressed={selectedPhase === option.phase}
                    key={option.label}
                    onClick={() => setSelectedPhase(option.phase)}
                  >
                    <span>{option.label}</span>
                    <strong>{phaseCount(page, option.phase)}</strong>
                  </button>
                ))}
              </div>
              <label className="field memory-search" htmlFor={searchId}>
                <span className="field__label">搜索项目记忆</span>
                <input
                  id={searchId}
                  type="search"
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder="标题、摘要、事件类型或对象"
                />
              </label>
            </div>

            {noProjectEvents ? (
              <div className="empty-state memory-empty">
                <strong>项目尚未产生事件</strong>
                <p>导入需求或配置项目后，变更会按时间顺序出现在这里。</p>
              </div>
            ) : null}

            {!noProjectEvents && noVisibleEvents ? (
              <div className="empty-state memory-empty">
                <strong>暂无可显示的变更</strong>
                <p>当前有效变更被隐藏；可以打开“显示系统心跳”检查原始活动。</p>
              </div>
            ) : null}

            {!noProjectEvents && !noVisibleEvents && filteredItems.length === 0 ? (
              <div className="empty-state memory-empty">
                <strong>当前筛选没有匹配结果</strong>
                <p>调整阶段筛选或搜索关键词，也可以继续加载更早的历史事件。</p>
              </div>
            ) : null}

            {filteredItems.length > 0 ? (
              <ol className="memory-timeline" aria-label="项目事件时间线">
                {filteredItems.map((item) => (
                  <li className={`memory-entry memory-entry--${item.phase}`} key={item.eventId}>
                    <div className="memory-entry__rail" aria-hidden="true">
                      <span />
                    </div>
                    <article className="memory-entry__card">
                      <header className="memory-entry__meta">
                        <span className={`memory-phase memory-phase--${item.phase}`}>
                          {phaseLabels[item.phase]}
                        </span>
                        <code title={item.eventType}>
                          {eventTypeLabel(item.eventType)}
                        </code>
                        <time dateTime={item.occurredAt}>{formatTime(item.occurredAt)}</time>
                      </header>
                      <h3>{item.title}</h3>
                      <p>{item.summary}</p>
                      {item.reference ? (
                        <div className="memory-reference">
                          <span>{item.reference.label}</span>
                          <code>{item.reference.id}</code>
                        </div>
                      ) : null}
                    </article>
                  </li>
                ))}
              </ol>
            ) : null}

            <div className="memory-pagination">
              {page.nextCursor !== null ? (
                <button
                  className="button button--secondary"
                  type="button"
                  onClick={() => void loadMore()}
                  disabled={loadingMore}
                >
                  {loadingMore ? '正在加载…' : '加载更多历史事件'}
                </button>
              ) : (
                <span>已加载当前范围内的全部事件</span>
              )}
            </div>
          </section>
        </>
      ) : null}
    </section>
  );
}
