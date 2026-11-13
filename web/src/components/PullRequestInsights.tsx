import type {
  PullRequestCheck,
  PullRequestInsights as PullRequestInsightsData,
  PullRequestReviewState,
} from '../api/types';

const checkStatusLabels: Record<PullRequestCheck['status'], string> = {
  queued: '排队中',
  in_progress: '运行中',
  completed: '已完成',
  waiting: '等待中',
  requested: '已请求',
  pending: '等待中',
  unknown: '未知状态',
};

const checkConclusionLabels: Record<NonNullable<PullRequestCheck['conclusion']>, string> = {
  success: '通过',
  failure: '失败',
  neutral: '中性',
  cancelled: '已取消',
  skipped: '已跳过',
  timed_out: '超时',
  action_required: '需要操作',
  startup_failure: '启动失败',
  stale: '已过期',
  unknown: '未知结论',
};

const reviewStateLabels: Record<PullRequestReviewState, string> = {
  approved: '已批准',
  changes_requested: '要求修改',
  commented: '已评论',
  dismissed: '已驳回',
  pending: '等待中',
  unknown: '未知状态',
};

function failedChecks(insights: PullRequestInsightsData) {
  return insights.checks.filter((check) =>
    ['failure', 'cancelled', 'timed_out', 'action_required', 'startup_failure'].includes(
      check.conclusion ?? '',
    ),
  ).length;
}

function pendingChecks(insights: PullRequestInsightsData) {
  return insights.checks.filter((check) => check.status !== 'completed').length;
}

function checksBadge(insights: PullRequestInsightsData) {
  const failed = failedChecks(insights);
  const pending = pendingChecks(insights);
  if (insights.checks.length === 0) {
    return { label: '无 Checks', tone: 'none' };
  }
  if (failed > 0) {
    return { label: `${failed} 项 Checks 失败`, tone: 'failure' };
  }
  if (pending > 0) {
    return { label: `${pending} 项 Checks 进行中`, tone: 'pending' };
  }
  return { label: 'Checks 全部通过', tone: 'success' };
}

function reviewBadge(insights: PullRequestInsightsData) {
  const changesRequested = insights.reviews.filter(
    (review) => review.state === 'changes_requested',
  ).length;
  const approved = insights.reviews.filter((review) => review.state === 'approved').length;
  if (changesRequested > 0) {
    return { label: `${changesRequested} 项要求修改`, tone: 'failure' };
  }
  if (approved > 0) {
    return { label: `${approved} 项 Review 已批准`, tone: 'success' };
  }
  if (insights.reviews.length > 0) {
    return { label: `${insights.reviews.length} 条 Review`, tone: 'neutral' };
  }
  return { label: '暂无 Review', tone: 'none' };
}

function formatSyncedAt(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString('zh-CN', { hour12: false });
}

export function PullRequestInsights({ insights }: { insights: PullRequestInsightsData }) {
  const checks = checksBadge(insights);
  const reviews = reviewBadge(insights);

  return (
    <section className="pr-insights" aria-label="GitHub PR 验收信息">
      <header className="pr-insights__header">
        <div className="pr-insights__badges">
          <span className={`pr-insights__badge pr-insights__badge--${checks.tone}`}>
            {checks.label}
          </span>
          <span className={`pr-insights__badge pr-insights__badge--${reviews.tone}`}>
            {reviews.label}
          </span>
          <span className="pr-insights__badge pr-insights__badge--neutral">
            {insights.reviewComments.length} 条行内评论
          </span>
        </div>
        <small>同步于 {formatSyncedAt(insights.syncedAt)}</small>
      </header>

      <details className="pr-insights__details">
        <summary>查看 Checks、Review 与评论</summary>
        <div className="pr-insights__grid">
          <section className="pr-insights__section">
            <h4>Checks <span>{insights.checks.length}</span></h4>
            {insights.checks.length === 0 ? (
              <p className="pr-insights__empty">此提交没有 GitHub Check Run。</p>
            ) : (
              <ul className="pr-insights__list">
                {insights.checks.map((check, index) => (
                  <li key={`${check.name}-${index}`}>
                    <div>
                      <strong>{check.name}</strong>
                      <span className={`pr-insights__state pr-insights__state--${check.status}`}>
                        {checkStatusLabels[check.status]}
                      </span>
                    </div>
                    <small>
                      {check.appName ?? 'GitHub'}
                      {check.conclusion
                        ? ` · ${checkConclusionLabels[check.conclusion]}`
                        : ''}
                    </small>
                    {check.detailsUrl ? (
                      <a href={check.detailsUrl} target="_blank" rel="noreferrer">
                        查看运行
                      </a>
                    ) : null}
                  </li>
                ))}
              </ul>
            )}
          </section>

          <section className="pr-insights__section">
            <h4>Review <span>{insights.reviews.length}</span></h4>
            {insights.reviews.length === 0 ? (
              <p className="pr-insights__empty">尚无 Review 结论。</p>
            ) : (
              <ul className="pr-insights__list">
                {insights.reviews.map((review, index) => (
                  <li key={`${review.author}-${index}`}>
                    <div>
                      <strong>{review.author}</strong>
                      <span className="pr-insights__state">
                        {reviewStateLabels[review.state]}
                      </span>
                    </div>
                    {review.body ? <p>{review.body}</p> : null}
                    {review.htmlUrl ? (
                      <a href={review.htmlUrl} target="_blank" rel="noreferrer">
                        在 GitHub 查看
                      </a>
                    ) : null}
                  </li>
                ))}
              </ul>
            )}
          </section>

          <section className="pr-insights__section pr-insights__section--wide">
            <h4>行内评论 <span>{insights.reviewComments.length}</span></h4>
            {insights.reviewComments.length === 0 ? (
              <p className="pr-insights__empty">尚无行内 Review 评论。</p>
            ) : (
              <ul className="pr-insights__list">
                {insights.reviewComments.map((comment, index) => (
                  <li key={`${comment.path}-${comment.line ?? 0}-${index}`}>
                    <div>
                      <strong>{comment.path}{comment.line ? `:${comment.line}` : ''}</strong>
                      <span className="pr-insights__state">{comment.author}</span>
                    </div>
                    <p>{comment.body}</p>
                    {comment.htmlUrl ? (
                      <a href={comment.htmlUrl} target="_blank" rel="noreferrer">
                        查看评论
                      </a>
                    ) : null}
                  </li>
                ))}
              </ul>
            )}
          </section>
        </div>
        <p className="pr-insights__limit">
          ColdStart 只读取 GitHub 第一页数据，每类最多 100 项；不会回复评论、重跑 Checks 或写回任何状态。
        </p>
      </details>
    </section>
  );
}
