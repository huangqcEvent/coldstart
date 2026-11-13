/**
 * 发布中心页面。
 *
 * 展示当前任务交付状态和人工验收入口；MVP 不自动部署、打包、提审或创建 Release Tag。
 */
import { useEffect, useState } from 'react';

import * as integrationApi from '../api/integrationApi';
import type {
  CommandError,
  ReleaseCenter,
  WorkItemStatus,
  WorkItemView,
} from '../api/types';
import { ErrorBanner } from './ErrorBanner';

interface ReleaseCenterViewProps {
  path: string;
  onEnterInbox: () => void;
}

interface ReleaseWorkListProps {
  eyebrow: string;
  title: string;
  description: string;
  items: WorkItemView[];
  emptyText: string;
}

const workStatusLabels: Record<WorkItemStatus, string> = {
  draft: '草稿',
  ready: '待领取',
  claimed: '已领取',
  in_progress: '进行中',
  submitted: '待验收',
  done: '已完成',
  blocked: '阻塞',
  failed: '失败',
  reopened: '重新打开',
  cancelled: '已取消',
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

function ReleaseWorkCard({ item }: { item: WorkItemView }) {
  const { task, workItem } = item;

  return (
    <article className="release-work-card">
      <header>
        <div>
          <strong>{task.title}</strong>
          <code>{task.taskKey}</code>
        </div>
        <span
          className={`release-status release-status--${workItem.status}`}
        >
          {workStatusLabels[workItem.status]}
        </span>
      </header>
      <p>{task.goal}</p>
      <dl className="release-work-details">
        <div>
          <dt>范围</dt>
          <dd>{task.scope.join('；') || '未限定'}</dd>
        </div>
        <div>
          <dt>明确不做</dt>
          <dd>{task.outOfScope.join('；') || '未声明'}</dd>
        </div>
        <div>
          <dt>关键路径</dt>
          <dd>{task.criticalPath ? '是' : '否'}</dd>
        </div>
        <div>
          <dt>验收方式</dt>
          <dd>
            {task.acceptancePolicy === 'human_acceptance'
              ? '人工验收'
              : 'Codex 上报验收'}
          </dd>
        </div>
      </dl>
      {task.acceptanceCriteria.length ? (
        <div className="release-work-criteria">
          <span>验收标准</span>
          <ul>
            {task.acceptanceCriteria.map((criterion) => (
              <li key={criterion.id}>{criterion.text}</li>
            ))}
          </ul>
        </div>
      ) : null}
    </article>
  );
}

function ReleaseWorkList({
  eyebrow,
  title,
  description,
  items,
  emptyText,
}: ReleaseWorkListProps) {
  return (
    <section className="panel release-section" aria-labelledby={`release-${title}`}>
      <div className="panel__head">
        <div>
          <p className="panel__eyebrow">{eyebrow}</p>
          <h2 id={`release-${title}`}>{title}</h2>
          <p className="release-section__description">{description}</p>
        </div>
        <span className="count-badge">{items.length}</span>
      </div>
      {items.length ? (
        <div className="release-work-list">
          {items.map((item) => (
            <ReleaseWorkCard item={item} key={item.workItem.id} />
          ))}
        </div>
      ) : (
        <div className="empty-state">
          <strong>当前没有相关条目</strong>
          <p>{emptyText}</p>
        </div>
      )}
    </section>
  );
}

export function ReleaseCenterView({
  path,
  onEnterInbox,
}: ReleaseCenterViewProps) {
  const [release, setRelease] = useState<ReleaseCenter | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<CommandError | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    integrationApi
      .readIntegrationWorkspace(path)
      .then((workspace) => {
        if (!cancelled) {
          setRelease(workspace.release);
        }
      })
      .catch((loadError: unknown) => {
        if (!cancelled) {
          setError(normalizeCommandError(loadError));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [path]);

  const completed = release
    ? [
        ...release.prerequisites,
        ...release.milestones,
        ...release.approvalGates,
        ...release.humanTasks,
      ].filter((item) => item.workItem.status === 'done').length
    : 0;
  const total = release
    ? release.prerequisites.length +
      release.milestones.length +
      release.approvalGates.length +
      release.humanTasks.length
    : 0;

  return (
    <section className="content-view release-view" aria-busy={loading}>
      <header className="content-view__header">
        <div>
          <p className="eyebrow">发布准备</p>
          <h1>发布中心</h1>
          <p className="content-view__lead">
            汇总上架、提审、签名与人工审批的当前状态，让 Codex 在开发计划中提前避开外部等待。
          </p>
        </div>
        <button
          className="button button--primary"
          type="button"
          onClick={onEnterInbox}
        >
          进入人工 Inbox
        </button>
      </header>

      {error ? <ErrorBanner error={error} onDismiss={() => setError(null)} /> : null}

      <aside className="release-guardrail">
        <span aria-hidden="true">只读</span>
        <div>
          <strong>只汇总状态，不部署、不打包、不提审</strong>
          <p>
            外部操作仍由人在真实平台完成；这里的完成状态只来自人工 Inbox 的明确确认。
          </p>
        </div>
      </aside>

      {loading ? (
        <div className="empty-state">正在汇总发布条件与里程碑…</div>
      ) : release ? (
        <>
          <div className="plan-metrics release-metrics">
            <article>
              <span>发布前置条件</span>
              <strong>{release.prerequisites.length}</strong>
            </article>
            <article>
              <span>发布里程碑</span>
              <strong>{release.milestones.length}</strong>
            </article>
            <article>
              <span>审批与人工任务</span>
              <strong>
                {release.approvalGates.length + release.humanTasks.length}
              </strong>
            </article>
            <article>
              <span>已完成 / 总计</span>
              <strong>
                {completed} / {total}
              </strong>
            </article>
          </div>

          <ReleaseWorkList
            eyebrow="外部前置条件"
            title="发布前置条件"
            description="开发者账号、签名、备案、提审材料等需要提前启动的外部工作。"
            items={release.prerequisites}
            emptyText="当前计划没有声明发布前置条件。"
          />
          <ReleaseWorkList
            eyebrow="交付节点"
            title="发布里程碑"
            description="到达可发布状态前必须完成的关键计划节点。"
            items={release.milestones}
            emptyText="当前计划没有生成发布里程碑。"
          />
          <ReleaseWorkList
            eyebrow="人工审批"
            title="人工审批闸门"
            description="必须由人确认后才能继续推进的验收与授权节点。"
            items={release.approvalGates}
            emptyText="当前计划没有人工审批闸门。"
          />
          <ReleaseWorkList
            eyebrow="外部操作"
            title="人工外部任务"
            description="需要在控制台、应用商店或第三方平台实际完成的操作。"
            items={release.humanTasks}
            emptyText="当前计划没有其他人工外部任务。"
          />
        </>
      ) : null}
    </section>
  );
}
