/**
 * 多仓库交付总览界面。
 *
 * 按仓库展示任务、验证、Git、PR、Checks、Review 与风险建议；所有数据来自本地
 * 聚合快照，刷新按钮不会触发后台轮询。
 */
import type {
  CommandError,
  DeliveryOverviewView,
  DeliveryPullRequestView,
  DeliveryRepositoryState,
  RepositoryDeliveryView,
} from '../api/types';

interface DeliveryOverviewProps {
  overview: DeliveryOverviewView | null;
  loading: boolean;
  error: CommandError | null;
  onRetry: () => void;
  onEnterIntegration: () => void;
}

const repositoryStateLabels: Record<DeliveryRepositoryState, string> = {
  attention: '需关注',
  active: '推进中',
  ready: '可领取',
  complete: '已完成',
  idle: '暂无任务',
};

const pullRequestStateLabels: Record<DeliveryPullRequestView['state'], string> = {
  open: '开放',
  closed: '已关闭',
  merged: '已合并',
};

const checksStateLabels: Record<
  NonNullable<DeliveryPullRequestView['checks']>['state'],
  string
> = {
  none: '无 Checks',
  pending: 'Checks 等待中',
  success: 'Checks 通过',
  failure: 'Checks 失败',
};

function formatTime(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return new Intl.DateTimeFormat('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  }).format(date);
}

function pullRequestLabel(pullRequest: DeliveryPullRequestView) {
  if (pullRequest.state !== 'open') {
    return `PR #${pullRequest.number} · ${pullRequestStateLabels[pullRequest.state]}`;
  }
  if (!pullRequest.checks) {
    return `PR #${pullRequest.number} · 尚未同步验收`;
  }
  return `PR #${pullRequest.number} · ${checksStateLabels[pullRequest.checks.state]}`;
}

function pullRequestDetails(pullRequest: DeliveryPullRequestView) {
  const details: string[] = [];
  if (pullRequest.conflictPathCount > 0) {
    details.push(`冲突 ${pullRequest.conflictPathCount} 个文件`);
  } else if (pullRequest.mergeability === 'conflicted') {
    details.push('合并冲突');
  } else if (pullRequest.mergeability) {
    const mergeabilityLabels: Record<DeliveryPullRequestView['mergeability'] & string, string> = {
      mergeable: '可合并',
      conflicted: '合并冲突',
      blocked: '合并受阻',
      behind: '落后目标分支',
      unstable: '状态不稳定',
      draft: '草稿 PR',
      checking: '合并状态计算中',
      unknown: '合并状态未知',
    };
    details.push(mergeabilityLabels[pullRequest.mergeability]);
  }
  if (pullRequest.changesRequestedReviewCount > 0) {
    details.push(`${pullRequest.changesRequestedReviewCount} 条修改意见`);
  }
  if (pullRequest.reviewStatus) {
    const reviewLabels: Record<NonNullable<DeliveryPullRequestView['reviewStatus']>, string> = {
      not_required: '无需 AI Review',
      running: 'AI Review 进行中',
      passed: 'AI Review 通过',
      blocked: 'AI Review 阻断',
      failed: 'AI Review 失败',
      waived: 'AI Review 已豁免',
      interrupted: 'AI Review 中断',
    };
    details.push(reviewLabels[pullRequest.reviewStatus]);
  }
  return details.join(' · ') || '尚未同步验收快照';
}

function RepositoryCard({ repository }: { repository: RepositoryDeliveryView }) {
  const { tasks } = repository;
  const completion = tasks.total > 0
    ? Math.round((tasks.done / tasks.total) * 100)
    : 0;
  const verificationMissing = tasks.total > 0 && repository.verificationCommandCount === 0;
  const gitSync = repository.latestGitSync;
  const pullRequest = repository.pullRequest;
  const taskCounts = [
    ['可领取', tasks.ready],
    ['执行中', tasks.active],
    ['阻塞', tasks.blocked],
    ['失败', tasks.failed],
    ['待验收', tasks.submitted],
    ...(tasks.reopened > 0 ? [['已重开', tasks.reopened] as const] : []),
    ...(tasks.draft > 0 ? [['草稿', tasks.draft] as const] : []),
  ] as const;

  return (
    <article
      className={`delivery-card delivery-card--${repository.state}`}
      aria-label={`${repository.repositoryName}交付状态`}
    >
      <header className="delivery-card__head">
        <div>
          <span className="delivery-card__key">{repository.repositoryKey}</span>
          <h3>{repository.repositoryName}</h3>
        </div>
        <span className={`delivery-card__state delivery-card__state--${repository.state}`}>
          {repositoryStateLabels[repository.state]}
        </span>
      </header>

      <p className="delivery-card__remote" title={repository.remoteUrl}>
        {repository.remoteUrl}
      </p>

      <div className="delivery-card__progress-copy">
        <strong>{tasks.done}/{tasks.total} · {completion}%</strong>
        <span>{repository.defaultBranch} · {tasks.total} 个有效任务</span>
      </div>
      <div
        className="delivery-card__progress"
        role="progressbar"
        aria-label={`${repository.repositoryName}任务完成度`}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={completion}
      >
        <span style={{ width: `${completion}%` }} />
      </div>

      <ul className="delivery-card__counts" aria-label={`${repository.repositoryName}任务状态`}>
        {taskCounts.map(([label, count]) => (
          <li key={label}>
            <span>{label}</span>
            <strong>{count}</strong>
          </li>
        ))}
      </ul>

      <dl className="delivery-card__facts">
        <div>
          <dt>验证命令</dt>
          <dd className={verificationMissing ? 'delivery-card__risk' : undefined}>
            {repository.verificationCommandCount > 0
              ? `${repository.verificationCommandCount} 条已配置`
              : '未配置'}
          </dd>
        </div>
        <div>
          <dt>最近推送</dt>
          <dd className={gitSync?.status === 'failed' ? 'delivery-card__risk' : undefined}>
            {gitSync ? (
              <>
                {gitSync.status === 'failed' ? '推送失败' : '推送成功'}
                {' · '}
                <time dateTime={gitSync.createdAt}>{formatTime(gitSync.createdAt)}</time>
              </>
            ) : '暂无记录'}
          </dd>
        </div>
        <div>
          <dt>最近 PR</dt>
          <dd className={pullRequest?.conflictPathCount ? 'delivery-card__risk' : undefined}>
            {pullRequest ? (
              <>
                <a
                  href={pullRequest.htmlUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                  title={pullRequest.title}
                >
                  {pullRequestLabel(pullRequest)}
                </a>
                <small>{pullRequestDetails(pullRequest)}</small>
              </>
            ) : '暂无 PR'}
          </dd>
        </div>
      </dl>

      <footer className="delivery-card__next">
        <span>下一步</span>
        <p>{repository.nextAction}</p>
      </footer>
    </article>
  );
}

export function DeliveryOverview({
  overview,
  loading,
  error,
  onRetry,
  onEnterIntegration,
}: DeliveryOverviewProps) {
  if (loading && !overview) {
    return (
      <section className="delivery-overview" aria-labelledby="delivery-overview-title">
        <header className="delivery-overview__header">
          <div>
            <p className="panel__eyebrow">只读聚合 · 本地事实</p>
            <h2 id="delivery-overview-title">多仓库交付总览</h2>
          </div>
        </header>
        <div className="delivery-overview__state" role="status" aria-live="polite">
          正在聚合多仓库交付状态…
        </div>
      </section>
    );
  }

  if (error) {
    return (
      <section className="delivery-overview" aria-labelledby="delivery-overview-title">
        <header className="delivery-overview__header">
          <div>
            <p className="panel__eyebrow">只读聚合 · 本地事实</p>
            <h2 id="delivery-overview-title">多仓库交付总览</h2>
          </div>
        </header>
        <div className="delivery-overview__error" role="alert">
          <div>
            <strong>交付总览读取失败</strong>
            <p>{error.message}</p>
          </div>
          <button className="button button--secondary" type="button" onClick={onRetry}>
            重新读取
          </button>
        </div>
      </section>
    );
  }

  if (!overview) {
    return null;
  }

  const summary = overview.repositories.reduce(
    (totals, repository) => ({
      total: totals.total + repository.tasks.total,
      done: totals.done + repository.tasks.done,
      ready: totals.ready + repository.tasks.ready,
      active: totals.active + repository.tasks.active + repository.tasks.submitted,
      attention: totals.attention + (repository.state === 'attention' ? 1 : 0),
      openPullRequests:
        totals.openPullRequests +
        (repository.pullRequest?.state === 'open' ? 1 : 0),
    }),
    { total: 0, done: 0, ready: 0, active: 0, attention: 0, openPullRequests: 0 },
  );
  const completion = summary.total > 0
    ? Math.round((summary.done / summary.total) * 100)
    : 0;

  return (
    <section className="delivery-overview" aria-labelledby="delivery-overview-title">
      <header className="delivery-overview__header">
        <div>
          <p className="panel__eyebrow">只读聚合 · 本地事实</p>
          <h2 id="delivery-overview-title">多仓库交付总览</h2>
          <p>
            按仓库汇总当前已发布计划、验证配置、最近推送、PR 与 Review 快照；不会触发网络请求。
          </p>
        </div>
        <div className="delivery-overview__meta">
          <span>{overview.planRevisionNumber ? `计划 v${overview.planRevisionNumber}` : '计划未发布'}</span>
          <span>{overview.repositories.length}/5 个仓库已接入</span>
          {loading ? <span className="delivery-overview__loading">正在刷新…</span> : null}
        </div>
      </header>

      {overview.unassignedAgentTaskCount > 0 ? (
        <div className="delivery-overview__warning" role="status">
          有 {overview.unassignedAgentTaskCount} 个编码任务未关联已配置仓库；请检查计划任务或仓库映射。
        </div>
      ) : null}

      {overview.repositories.length === 0 ? (
        <div className="delivery-overview__empty">
          <strong>尚未映射仓库</strong>
          <p>
            配置 server、admin、web、app 或 mini_program 后，这里会按仓库汇总任务、验证和 PR 状态。
          </p>
          <button className="button button--secondary" type="button" onClick={onEnterIntegration}>
            前往仓库与能力
          </button>
        </div>
      ) : (
        <>
          <div className="delivery-summary" aria-label="多仓库交付摘要">
            <article>
              <span>有效任务</span>
              <strong>{summary.done}/{summary.total}</strong>
              <small>完成度 {completion}%</small>
            </article>
            <article>
              <span>可领取</span>
              <strong>{summary.ready}</strong>
              <small>由 Codex 主动领取</small>
            </article>
            <article>
              <span>执行中</span>
              <strong>{summary.active}</strong>
              <small>含待验收任务</small>
            </article>
            <article className={summary.attention > 0 ? 'delivery-summary__attention' : undefined}>
              <span>需关注仓库</span>
              <strong>{summary.attention}</strong>
              <small>{summary.attention > 0 ? '优先处理阻塞与风险' : '当前无阻断项'}</small>
            </article>
            <article>
              <span>开放 PR</span>
              <strong>{summary.openPullRequests}</strong>
              <small>只读展示本地快照</small>
            </article>
          </div>

          <div className="delivery-grid">
            {overview.repositories.map((repository) => (
              <RepositoryCard key={repository.repositoryKey} repository={repository} />
            ))}
          </div>
        </>
      )}
    </section>
  );
}
