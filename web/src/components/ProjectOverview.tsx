import { useCallback, useEffect, useState } from 'react';

import * as deliveryApi from '../api/deliveryApi';
import type {
  CommandError,
  DeliveryOverviewView,
  ProjectWorkspace,
} from '../api/types';
import { DeliveryOverview } from './DeliveryOverview';

interface ProjectOverviewProps {
  workspace: ProjectWorkspace;
  busy: boolean;
  onRefresh: () => void;
  onEnterRequirements: () => void;
  onEnterPlan: () => void;
  onEnterIntegration: () => void;
}

function normalizeCommandError(error: unknown): CommandError {
  if (typeof error === 'object' && error !== null) {
    const candidate = error as Record<string, unknown>;
    if (typeof candidate.code === 'string' && typeof candidate.message === 'string') {
      return { code: candidate.code, message: candidate.message };
    }
  }

  return { code: 'unknown', message: String(error) };
}

export function ProjectOverview({
  workspace,
  busy,
  onRefresh,
  onEnterRequirements,
  onEnterPlan,
  onEnterIntegration,
}: ProjectOverviewProps) {
  const [deliveryOverview, setDeliveryOverview] = useState<DeliveryOverviewView | null>(null);
  const [deliveryLoading, setDeliveryLoading] = useState(true);
  const [deliveryError, setDeliveryError] = useState<CommandError | null>(null);

  const loadDelivery = useCallback(async () => {
    setDeliveryLoading(true);
    setDeliveryError(null);
    try {
      const loaded = await deliveryApi.readDeliveryOverview(workspace.status.rootPath);
      setDeliveryOverview(loaded);
    } catch (error) {
      setDeliveryOverview(null);
      setDeliveryError(normalizeCommandError(error));
    } finally {
      setDeliveryLoading(false);
    }
  }, [workspace.status.rootPath]);

  useEffect(() => {
    void loadDelivery();
  }, [loadDelivery]);

  const openInbox = workspace.inbox.filter(
    (request) => request.status === 'open',
  ).length;
  const draftLabel = workspace.draft
    ? `草稿 v${workspace.draft.revisionNumber}`
    : '暂无草稿';
  const publishedLabel = workspace.published
    ? `已发布 v${workspace.published.revisionNumber}`
    : '尚未发布';
  const planDone = workspace.plan.workItems.filter(
    (item) => item.workItem.status === 'done',
  ).length;
  const planTotal = workspace.plan.workItems.length;
  const planLabel = planTotal > 0
    ? `${planDone}/${planTotal}`
    : workspace.plan.published
      ? '已发布'
      : workspace.plan.draft
        ? '待发布'
        : '未生成';
  const planPercent = planTotal > 0 ? Math.round((planDone / planTotal) * 100) : 0;
  const nextStep =
    workspace.sources.length === 0
      ? '导入需求文档，让模型提出可追溯的结构化蓝图'
      : openInbox > 0
        ? '在人工 Inbox 处理阻塞问题，再继续推进蓝图'
        : workspace.draft
          ? '检查并发布当前蓝图草稿'
          : workspace.published
            ? workspace.plan.draft
              ? '检查并发布开发计划，随后让 Codex 通过 MCP 主动领取'
              : workspace.plan.published
                ? planTotal > 0 && planDone === planTotal
                  ? '计划已全部完成；进入发布中心核对上架、提审与审批状态'
                  : '计划执行中；Codex 通过 MCP 领取任务，仓库分支与推送风险可实时观察'
                : '编译开发计划并发布任务 DAG'
            : '在需求导入页生成蓝图草稿';

  return (
    <section className="project-view" aria-labelledby="project-title">
      <header className="project-view__header">
        <div>
          <p className="eyebrow">项目总览 · 本地已连接</p>
          <h1 id="project-title">{workspace.status.name}</h1>
          <p className="project-view__lead">
            {workspace.status.rootPath} · {workspace.status.eventCount} 个事件
          </p>
        </div>
        <div className="view-actions">
          <button
            className="button button--secondary"
            type="button"
            onClick={() => {
              onRefresh();
              void loadDelivery();
            }}
            disabled={busy || deliveryLoading}
          >
            刷新状态
          </button>
          <button
            className="button button--primary"
            type="button"
            onClick={workspace.published ? onEnterPlan : onEnterRequirements}
          >
            {workspace.published ? '进入计划看板' : '进入需求导入'}
          </button>
        </div>
      </header>

      <div className="metrics-grid metrics-grid--five" aria-label="项目流程指标">
        <article className="metric-card">
          <p>来源文档</p>
          <strong>{workspace.sources.length}</strong>
          <small>已固化到项目目录</small>
        </article>
        <article className="metric-card">
          <p>蓝图草稿</p>
          <strong>{draftLabel}</strong>
          <small>{workspace.draft ? '可继续人工编辑' : '导入需求后生成'}</small>
        </article>
        <article className="metric-card">
          <p>发布版本</p>
          <strong>{publishedLabel}</strong>
          <small>{workspace.published ? '当前有效蓝图' : '发布前需通过校验'}</small>
        </article>
        <article className="metric-card metric-card--attention">
          <p>待处理 Inbox</p>
          <strong>{openInbox}</strong>
          <small>{openInbox > 0 ? '需要人工提供信息' : '当前无阻塞项'}</small>
        </article>
        <article className="metric-card">
          <p>计划进度</p>
          <strong>{planLabel}</strong>
          <div className="metric-card__progress" aria-hidden="true">
            <span style={{ width: `${planPercent}%` }} />
          </div>
          <small>{planTotal > 0 ? '任务状态实时观察' : '从已发布蓝图编译'}</small>
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

      <DeliveryOverview
        overview={deliveryOverview}
        loading={deliveryLoading}
        error={deliveryError}
        onRetry={() => void loadDelivery()}
        onEnterIntegration={onEnterIntegration}
      />

      <div className="project-view__footer">
        <span className="signal-line" aria-hidden="true" />
        <div>
          <strong>建议下一步</strong>
          <span>{nextStep}</span>
        </div>
      </div>
    </section>
  );
}
