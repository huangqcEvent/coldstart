/**
 * Pull Request 验收快照面板。
 *
 * 展示 Checks、Review、合并就绪、冲突与部署状态，并提供人工刷新和草稿 PR 操作；
 * 不自动合并、关闭或重跑外部任务。
 */
import type {
  PullRequestCheck,
  PullRequestDeployment,
  PullRequestInsights as PullRequestInsightsData,
  PullRequestMergeability,
  PullRequestRecord,
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

const mergeabilityLabels: Record<
  PullRequestMergeability,
  { label: string; tone: string }
> = {
  mergeable: { label: '合并状态：可合并', tone: 'success' },
  conflicted: { label: '合并冲突', tone: 'failure' },
  blocked: { label: '合并状态：暂不可合并', tone: 'failure' },
  behind: { label: '分支落后目标分支', tone: 'pending' },
  unstable: { label: '合并状态不稳定', tone: 'pending' },
  draft: { label: '草稿 PR', tone: 'none' },
  checking: { label: 'GitHub 正在计算合并状态', tone: 'pending' },
  unknown: { label: '合并状态未知', tone: 'none' },
};

const deploymentStateLabels: Record<PullRequestDeployment['state'], string> = {
  success: '部署成功',
  failure: '部署失败',
  error: '部署错误',
  inactive: '已停用',
  in_progress: '部署中',
  queued: '排队中',
  pending: '等待中',
  unknown: '状态未知',
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

function mergeabilityBadge(
  insights: PullRequestInsightsData,
  pullRequest: PullRequestRecord,
) {
  if (pullRequest.state === 'merged') {
    return { label: 'PR 已合并', tone: 'success' };
  }
  if (pullRequest.state === 'closed') {
    return { label: 'PR 已关闭', tone: 'none' };
  }
  return mergeabilityLabels[insights.mergeability];
}

function mergeReadiness(
  insights: PullRequestInsightsData,
  pullRequest: PullRequestRecord,
) {
  if (pullRequest.state === 'merged') {
    return { label: 'PR 已合并，无需合并前检查', tone: 'success', blockers: [] };
  }
  if (pullRequest.state === 'closed') {
    return { label: 'PR 已关闭，无需合并前检查', tone: 'none', blockers: [] };
  }

  const blockers = new Set<string>();
  if (insights.mergeability === 'conflicted') {
    blockers.add('存在合并冲突');
  } else if (insights.mergeability === 'blocked') {
    blockers.add('GitHub 标记为暂不可合并');
  } else if (insights.mergeability === 'behind') {
    blockers.add('任务分支落后于目标分支');
  } else if (insights.mergeability === 'unstable') {
    blockers.add('GitHub 合并状态不稳定');
  } else if (insights.mergeability === 'checking') {
    blockers.add('GitHub 正在计算合并状态');
  } else if (insights.mergeability === 'unknown') {
    blockers.add('GitHub 合并状态未知');
  }

  if (pullRequest.draft || insights.mergeability === 'draft') {
    blockers.add('PR 仍为草稿');
  }
  const failed = failedChecks(insights);
  if (failed > 0) {
    blockers.add(`${failed} 项 Checks 失败`);
  }
  const pending = pendingChecks(insights);
  if (pending > 0) {
    blockers.add(`${pending} 项 Checks 尚未完成`);
  }
  const changesRequested = insights.reviews.filter(
    (review) => review.state === 'changes_requested',
  ).length;
  if (changesRequested > 0) {
    blockers.add(`${changesRequested} 项 Review 要求修改`);
  }

  return blockers.size > 0
    ? {
        label: `合并前检查 · ${blockers.size} 项待人工确认`,
        tone: 'pending',
        blockers: [...blockers],
      }
    : {
        label: '合并前检查 · 当前快照未发现阻塞项',
        tone: 'success',
        blockers: [],
      };
}

function deploymentBadgeTone(state: PullRequestDeployment['state']) {
  if (state === 'failure' || state === 'error') {
    return 'failure';
  }
  if (state === 'in_progress' || state === 'queued' || state === 'pending') {
    return 'pending';
  }
  if (state === 'success') {
    return 'success';
  }
  return 'none';
}

function deploymentBadge(insights: PullRequestInsightsData) {
  const details = insights.deploymentDetails;
  if (details.state === 'not_checked') {
    return { label: '部署状态未同步', tone: 'none' };
  }
  if (details.state === 'no_deployments') {
    return { label: '无部署记录', tone: 'none' };
  }
  if (details.state === 'unavailable') {
    return { label: '部署状态不可用', tone: 'none' };
  }
  const failed = details.deployments.filter((deployment) =>
    ['failure', 'error'].includes(deployment.state),
  ).length;
  const pending = details.deployments.filter((deployment) =>
    ['in_progress', 'queued', 'pending'].includes(deployment.state),
  ).length;
  const unknown = details.deployments.filter(
    (deployment) => deployment.state === 'unknown',
  ).length;
  const inactive = details.deployments.filter(
    (deployment) => deployment.state === 'inactive',
  ).length;
  if (failed > 0) {
    return { label: `${failed} 个环境部署失败`, tone: 'failure' };
  }
  if (pending > 0) {
    return { label: `${pending} 个环境部署未结束`, tone: 'pending' };
  }
  if (unknown > 0) {
    return { label: `${unknown} 个环境部署状态未知`, tone: 'pending' };
  }
  if (inactive > 0) {
    return { label: '部分环境部署已停用', tone: 'none' };
  }
  return {
    label: `${details.deployments.length} 个环境部署成功`,
    tone: 'success',
  };
}

function deploymentDescription(insights: PullRequestInsightsData) {
  switch (insights.deploymentDetails.state) {
    case 'not_checked':
      return '这是阶段 16 之前的快照。再次刷新后会读取当前 PR 头提交的部署状态。';
    case 'no_deployments':
      return 'GitHub 未返回当前 PR 头提交的部署记录。';
    case 'unavailable':
      return '本次刷新未能读取 GitHub Deployment；PR 状态、Checks、Review 与评论不受影响。';
    case 'available':
    default:
      return '只展示当前 PR 头提交最近 10 条 GitHub Deployment，状态来自每个部署的最新记录。';
  }
}

function formatSyncedAt(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString('zh-CN', { hour12: false });
}

function conflictDiagnostic(insights: PullRequestInsightsData) {
  const details = insights.conflictDetails;
  switch (details.state) {
    case 'detected':
      return {
        label:
          details.paths.length > 0
            ? `本地定位到 ${details.paths.length} 个冲突文件`
            : 'Git 报告冲突，但未返回冲突路径',
        tone: 'failure',
        detail: '由本地 merge-tree 计算，未修改工作区；GitHub 状态仍是最终依据。',
      };
    case 'not_reproduced':
      return {
        label: '本地未复现冲突',
        tone: 'pending',
        detail:
          'GitHub 仍标记存在合并冲突。可能本地缺少最新目标分支或提交对象，请先同步后再次刷新；这不表示冲突已解决。',
      };
    case 'unavailable':
      return {
        label: '无法在本地定位冲突文件',
        tone: 'none',
        detail:
          '本地仓库缺少 PR 基准/头提交，或 Git 合并诊断失败。请先获取对应提交后再次刷新。',
      };
    case 'not_checked':
    default:
      return {
        label: '尚未执行本地冲突定位',
        tone: 'none',
        detail: '这是阶段 15 之前的快照。再次刷新后会尝试从本地仓库定位冲突路径。',
      };
  }
}

export function PullRequestInsights({
  insights,
  pullRequest,
}: {
  insights: PullRequestInsightsData;
  pullRequest: PullRequestRecord;
}) {
  const mergeability = mergeabilityBadge(insights, pullRequest);
  const readiness = mergeReadiness(insights, pullRequest);
  const checks = checksBadge(insights);
  const reviews = reviewBadge(insights);
  const conflicts = conflictDiagnostic(insights);
  const deployments = deploymentBadge(insights);
  const showConflicts =
    pullRequest.state === 'open' &&
    insights.mergeability === 'conflicted' &&
    insights.conflictDetails.state !== 'not_applicable';

  return (
    <section className="pr-insights" aria-label="GitHub PR 验收信息">
      <header className="pr-insights__header">
        <div className="pr-insights__badges">
          <span className={`pr-insights__badge pr-insights__badge--${mergeability.tone}`}>
            {mergeability.label}
          </span>
          <span className={`pr-insights__badge pr-insights__badge--${checks.tone}`}>
            {checks.label}
          </span>
          <span className={`pr-insights__badge pr-insights__badge--${reviews.tone}`}>
            {reviews.label}
          </span>
          <span className="pr-insights__badge pr-insights__badge--neutral">
            {insights.reviewComments.length} 条行内评论
          </span>
          <span className={`pr-insights__badge pr-insights__badge--${deployments.tone}`}>
            {deployments.label}
          </span>
        </div>
        <small>同步于 {formatSyncedAt(insights.syncedAt)}</small>
      </header>

      <div className={`pr-insights__readiness pr-insights__readiness--${readiness.tone}`}>
        <strong>{readiness.label}</strong>
        {readiness.blockers.length > 0 ? (
          <ul>
            {readiness.blockers.map((blocker) => (
              <li key={blocker}>{blocker}</li>
            ))}
          </ul>
        ) : (
          <span>只读提示，不构成自动合并许可；最终仍受分支保护、审批与 CI 规则约束。</span>
        )}
      </div>

      {showConflicts ? (
        <section
          className={`pr-insights__conflicts pr-insights__conflicts--${conflicts.tone}`}
          aria-label="本地冲突定位"
        >
          <header>
            <strong>{conflicts.label}</strong>
            <span>本地只读诊断</span>
          </header>
          <p>{conflicts.detail}</p>
          {insights.conflictDetails.state === 'detected' &&
          insights.conflictDetails.paths.length > 0 ? (
            <ul>
              {insights.conflictDetails.paths.map((path) => (
                <li key={path}>
                  <code>{path}</code>
                </li>
              ))}
            </ul>
          ) : null}
          {insights.conflictDetails.truncated ? (
            <small>仅展示前 100 个冲突路径，完整列表以本地 Git 状态为准。</small>
          ) : null}
        </section>
      ) : null}

      <section
        className={`pr-insights__deployments pr-insights__deployments--${deployments.tone}`}
        aria-label="GitHub 部署状态"
      >
        <header>
          <strong>部署状态</strong>
          <span>GitHub 只读</span>
        </header>
        <p>{deploymentDescription(insights)}</p>
        {insights.deploymentDetails.state === 'available' ? (
          <ul>
            {insights.deploymentDetails.deployments.map((deployment, index) => (
              <li key={`${deployment.environment}-${index}`}>
                <div>
                  <strong>{deployment.environment}</strong>
                  <span
                    className={`pr-insights__badge pr-insights__badge--${deploymentBadgeTone(deployment.state)}`}
                  >
                    {deploymentStateLabels[deployment.state]}
                  </span>
                </div>
                {deployment.description ? <p>{deployment.description}</p> : null}
                <small>
                  {deployment.statusUpdatedAt
                    ? `状态更新于 ${formatSyncedAt(deployment.statusUpdatedAt)}`
                    : '尚未获取部署状态记录'}
                </small>
              </li>
            ))}
          </ul>
        ) : null}
        {insights.deploymentDetails.truncated ? (
          <small>仅展示最近 10 条部署，完整记录请到 GitHub 查看。</small>
        ) : null}
        <small>部署成功不代表已获准发布或合并；ColdStart 不创建、审批、取消或重试部署。</small>
      </section>

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
          ColdStart 只读取 GitHub 第一页数据，每类最多 100 项；合并状态与评论只供人工验收，不会重跑 Checks、回复评论、合并 PR 或写回任何状态。
        </p>
      </details>
    </section>
  );
}
