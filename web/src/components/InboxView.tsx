/**
 * 人工 Inbox 页面。
 *
 * 集中展示待确认信息与人工任务，提交后刷新工作区；前端不自动回答业务问题。
 */
import { useId, useState } from 'react';

import type {
  InfoRequest,
  InfoRequestCategory,
  ProjectFact,
  WorkItemView,
} from '../api/types';

interface InboxViewProps {
  inbox: InfoRequest[];
  facts: ProjectFact[];
  humanTasks: WorkItemView[];
  busy: boolean;
  busyLabel: string | null;
  onResolve: (requestId: string, answer: string) => Promise<boolean>;
  onDismiss: (requestId: string, reason: string) => Promise<boolean>;
  onCompleteHuman: (
    workItemId: string,
    note: string,
    accepted: boolean,
  ) => Promise<boolean>;
}

const categoryOrder: InfoRequestCategory[] = [
  'information',
  'decision',
  'authorization',
  'approval',
  'external_operation',
];

const categoryLabels: Record<InfoRequestCategory, string> = {
  information: '信息补充',
  decision: '方案决策',
  authorization: '能力授权',
  approval: '人工审批',
  external_operation: '外部操作',
};

const workStatusLabels: Record<WorkItemView['workItem']['status'], string> = {
  draft: '草稿',
  ready: '待处理',
  claimed: '已领取',
  in_progress: '进行中',
  submitted: '待验收',
  done: '已完成',
  blocked: '阻塞',
  failed: '失败',
  reopened: '重新打开',
  cancelled: '已取消',
};

type HumanWorkPhase = 'prep' | 'release' | 'other';

interface HumanWorkGroup {
  label: string;
  items: WorkItemView[];
  prepCount: number;
  releaseCount: number;
  otherCount: number;
}

const selfHostedProviderLabel = '自建与内部系统';
const releaseComplianceLabel = '发布与合规';
const otherHumanWorkLabel = '其他人工任务';
const providerAliasSuffixes = [
  '实人认证',
  '电子签约',
  '云服务',
  '认证',
  '服务',
  '平台',
  '系统',
  '能力',
];

function humanWorkPhase(taskKey: string): HumanWorkPhase {
  if (taskKey.startsWith('capability-prep:')) {
    return 'prep';
  }
  if (taskKey.startsWith('release-prereq:')) {
    return 'release';
  }
  return 'other';
}

function providerFromPreparation(item: WorkItemView): string {
  const goalMatch = item.task.goal.match(/^确认\s*(.+?)\s*的账号/);
  if (goalMatch?.[1]) {
    const provider = goalMatch[1].trim();
    return provider.startsWith('自建') ? selfHostedProviderLabel : provider;
  }

  const titleMatch = item.task.title.match(/^准备(.+?)能力$/);
  if (titleMatch?.[1]) {
    return titleMatch[1].trim();
  }
  return otherHumanWorkLabel;
}

function aliasesForProvider(provider: string): string[] {
  if (provider === selfHostedProviderLabel) {
    return ['自建'];
  }

  const withoutParenthetical = provider.replace(/[（(][^）)]*[）)]/g, '').trim();
  const aliases = new Set<string>([provider, withoutParenthetical]);
  for (const segment of withoutParenthetical.split(/[+、/]/)) {
    const normalized = segment.trim();
    if (normalized.length >= 2) {
      aliases.add(normalized);
    }
  }

  for (const alias of [...aliases]) {
    for (const suffix of providerAliasSuffixes) {
      if (alias.endsWith(suffix) && alias.length > suffix.length + 1) {
        aliases.add(alias.slice(0, -suffix.length));
      }
    }
  }
  return [...aliases].filter((alias) => alias.length >= 2);
}

function groupHumanWork(humanTasks: WorkItemView[]): HumanWorkGroup[] {
  const preparationProviders = new Set(
    humanTasks
      .filter((item) => humanWorkPhase(item.task.taskKey) === 'prep')
      .map(providerFromPreparation)
      .filter((provider) => provider !== selfHostedProviderLabel)
      .filter((provider) => provider !== otherHumanWorkLabel),
  );

  const providerAliases = [...preparationProviders].map((provider) => ({
    provider,
    aliases: aliasesForProvider(provider),
  }));
  const groups = new Map<string, HumanWorkGroup>();

  for (const item of humanTasks) {
    const phase = humanWorkPhase(item.task.taskKey);
    let label: string;
    if (phase === 'prep') {
      label = providerFromPreparation(item);
    } else if (phase === 'release') {
      const releaseText = [
        item.task.title,
        item.task.goal,
        ...item.task.scope,
      ].join(' ');
      const matchedProvider = providerAliases
        .flatMap(({ provider, aliases }) =>
          aliases
            .filter((alias) => releaseText.includes(alias))
            .map((alias) => ({ provider, alias })),
        )
        .sort((left, right) => right.alias.length - left.alias.length)[0];
      label = matchedProvider?.provider ?? releaseComplianceLabel;
    } else {
      label = otherHumanWorkLabel;
    }

    const group = groups.get(label) ?? {
      label,
      items: [],
      prepCount: 0,
      releaseCount: 0,
      otherCount: 0,
    };
    group.items.push(item);
    if (phase === 'prep') {
      group.prepCount += 1;
    } else if (phase === 'release') {
      group.releaseCount += 1;
    } else {
      group.otherCount += 1;
    }
    groups.set(label, group);
  }

  const phaseOrder: Record<HumanWorkPhase, number> = {
    prep: 0,
    release: 1,
    other: 2,
  };
  return [...groups.values()]
    .map((group) => ({
      ...group,
      items: [...group.items].sort(
        (left, right) =>
          phaseOrder[humanWorkPhase(left.task.taskKey)] -
          phaseOrder[humanWorkPhase(right.task.taskKey)],
      ),
    }))
    .sort((left, right) => {
      if (left.label === releaseComplianceLabel) {
        return 1;
      }
      if (right.label === releaseComplianceLabel) {
        return -1;
      }
      return 0;
    });
}

function InboxRequestCard({
  request,
  busy,
  busyLabel,
  onResolve,
  onDismiss,
}: {
  request: InfoRequest;
  busy: boolean;
  busyLabel: string | null;
  onResolve: (requestId: string, answer: string) => Promise<boolean>;
  onDismiss: (requestId: string, reason: string) => Promise<boolean>;
}) {
  const answerId = useId();
  const dismissId = useId();
  const [answer, setAnswer] = useState('');
  const [dismissReason, setDismissReason] = useState('');
  const submitting = busyLabel === '正在提交 Inbox 回答';
  const dismissing = busyLabel === '正在关闭 Inbox 问题';

  return (
    <article className="inbox-card">
      <header className="inbox-card__head">
        <div>
          <span className={`inbox-badge inbox-badge--${request.category}`}>
            {categoryLabels[request.category]}
          </span>
          {request.blocking ? <span className="inbox-badge inbox-badge--blocking">阻塞</span> : null}
        </div>
        {request.dueAt ? <time dateTime={request.dueAt}>{request.dueAt}</time> : null}
      </header>
      <h3>{request.question}</h3>
      <dl className="inbox-details">
        <div>
          <dt>原因</dt>
          <dd>{request.reason || '未说明'}</dd>
        </div>
        <div>
          <dt>影响范围</dt>
          <dd>{request.impactScope || '未说明'}</dd>
        </div>
        {request.example ? (
          <div>
            <dt>示例</dt>
            <dd>{request.example}</dd>
          </div>
        ) : null}
        {request.defaultValue ? (
          <div>
            <dt>默认值</dt>
            <dd>{request.defaultValue}</dd>
          </div>
        ) : null}
      </dl>
      <label className="field" htmlFor={answerId}>
        <span>你的回答</span>
        <textarea
          id={answerId}
          value={answer}
          onChange={(event) => setAnswer(event.target.value)}
          placeholder={request.defaultValue ? `可采用默认值：${request.defaultValue}` : '填写可执行的明确信息'}
          aria-label={`回答：${request.question}`}
          disabled={busy}
          rows={3}
        />
      </label>
      <button
        className="button button--primary"
        type="button"
        onClick={() => void onResolve(request.id, answer.trim())}
        disabled={busy || answer.trim().length === 0}
        aria-label={`提交回答：${request.question}`}
      >
        {submitting ? '提交中…' : '提交回答'}
      </button>
      <details className="inbox-dismiss">
        <summary>问题已过时或不适用？关闭它</summary>
        <p className="field__hint">
          关闭后不会生成项目事实，但会保留原因和审计记录。
        </p>
        <label className="field" htmlFor={dismissId}>
          <span>关闭原因</span>
          <textarea
            id={dismissId}
            value={dismissReason}
            onChange={(event) => setDismissReason(event.target.value)}
            placeholder="说明为什么该问题已经过时、重复或不再适用"
            aria-label={`关闭原因：${request.question}`}
            disabled={busy}
            rows={2}
          />
        </label>
        <button
          className="button button--compact button--ghost"
          type="button"
          onClick={() => void onDismiss(request.id, dismissReason.trim())}
          disabled={busy || dismissReason.trim().length === 0}
          aria-label={`关闭问题：${request.question}`}
        >
          {dismissing ? '关闭中…' : '确认关闭问题'}
        </button>
      </details>
    </article>
  );
}

function HumanWorkCard({
  item,
  busy,
  busyLabel,
  onComplete,
}: {
  item: WorkItemView;
  busy: boolean;
  busyLabel: string | null;
  onComplete: (note: string, accepted: boolean) => Promise<boolean>;
}) {
  const noteId = useId();
  const [note, setNote] = useState('');
  const completing =
    busyLabel === '正在完成人工任务' || busyLabel === '正在标记人工阻塞';

  return (
    <article className="inbox-card inbox-card--work">
      <header className="inbox-card__head">
        <div>
          <span className="inbox-badge inbox-badge--external_operation">
            {item.task.kind === 'approval_gate' ? '人工审批' : '外部操作'}
          </span>
          <span className="inbox-badge">计划任务</span>
        </div>
        <span className={`inbox-work-status inbox-work-status--${item.workItem.status}`}>
          {workStatusLabels[item.workItem.status]}
        </span>
      </header>
      <h3>{item.task.title}</h3>
      <dl className="inbox-details">
        <div>
          <dt>原因</dt>
          <dd>{item.task.goal}</dd>
        </div>
        <div>
          <dt>范围</dt>
          <dd>{item.task.scope.join('；') || '未说明'}</dd>
        </div>
      </dl>
      <div className="inbox-work__acceptance">
        <span>验收标准</span>
        <ul>
          {item.task.acceptanceCriteria.map((criterion) => (
            <li key={criterion.id}>{criterion.text}</li>
          ))}
        </ul>
      </div>
      <label className="field" htmlFor={noteId}>
        <span>完成说明</span>
        <textarea
          id={noteId}
          value={note}
          onChange={(event) => setNote(event.target.value)}
          placeholder="说明账号、配置或审批已经可用；拒绝时写明缺口"
          aria-label={`完成说明：${item.task.title}`}
          disabled={busy}
          rows={3}
        />
      </label>
      <div className="view-actions inbox-work__actions">
        <button
          className="button button--primary"
          type="button"
          onClick={() => void onComplete(note.trim(), true)}
          disabled={busy || note.trim().length === 0}
          aria-label={`确认完成：${item.task.title}`}
        >
          {completing ? '处理中…' : '确认完成'}
        </button>
        <button
          className="button button--secondary"
          type="button"
          onClick={() => void onComplete(note.trim(), false)}
          disabled={busy || note.trim().length === 0}
          aria-label={`标记阻塞：${item.task.title}`}
        >
          标记阻塞
        </button>
      </div>
    </article>
  );
}

function HumanWorkProviderGroup({
  group,
  busy,
  busyLabel,
  onCompleteHuman,
}: {
  group: HumanWorkGroup;
  busy: boolean;
  busyLabel: string | null;
  onCompleteHuman: (
    workItemId: string,
    note: string,
    accepted: boolean,
  ) => Promise<boolean>;
}) {
  const [expanded, setExpanded] = useState(group.items.length <= 3);

  return (
    <section
      className="inbox-work-provider"
      role="region"
      aria-label={group.label}
    >
      <details
        className="inbox-work-group"
        open={expanded}
        onToggle={(event) => setExpanded(event.currentTarget.open)}
      >
        <summary className="inbox-work-group__summary">
          <span className="inbox-work-group__marker" aria-hidden="true">
            ▸
          </span>
          <span className="inbox-work-group__name">{group.label}</span>
          <span className="inbox-work-group__counts">
            {group.prepCount > 0 ? (
              <span className="inbox-work-group__count inbox-work-group__count--prep">
                研发准备 {group.prepCount}
              </span>
            ) : null}
            {group.releaseCount > 0 ? (
              <span className="inbox-work-group__count inbox-work-group__count--release">
                发布前置 {group.releaseCount}
              </span>
            ) : null}
            {group.otherCount > 0 ? (
              <span className="inbox-work-group__count">人工任务 {group.otherCount}</span>
            ) : null}
          </span>
        </summary>
        <div className="inbox-work-group__body">
          <div className="inbox-list">
            {group.items.map((item) => (
              <HumanWorkCard
                item={item}
                busy={busy}
                busyLabel={busyLabel}
                onComplete={(note, accepted) =>
                  onCompleteHuman(item.workItem.id, note, accepted)
                }
                key={item.workItem.id}
              />
            ))}
          </div>
        </div>
      </details>
    </section>
  );
}

export function InboxView({
  inbox,
  facts,
  humanTasks,
  busy,
  busyLabel,
  onResolve,
  onDismiss,
  onCompleteHuman,
}: InboxViewProps) {
  const openRequests = inbox.filter((request) => request.status === 'open');
  const resolvedRequests = inbox.filter((request) => request.status !== 'open');
  const humanWorkGroups = groupHumanWork(humanTasks);

  return (
    <section className="content-view" aria-labelledby="inbox-title">
      <header className="content-view__header">
        <div>
          <p className="eyebrow">人工确认 · 仅在必要时介入</p>
          <h1 id="inbox-title">人工 Inbox</h1>
          <p className="content-view__lead">
            开发只在缺少信息、需要决策或外部授权时中断。每条回答会成为可追溯的项目事实。
          </p>
        </div>
        <div
          className="inbox-counters"
          role="group"
          aria-label={`${openRequests.length} 条待补信息，${humanTasks.length} 条待办人工任务`}
        >
          <div className="inbox-counter inbox-counter--requests">
            <strong>{openRequests.length}</strong>
            <span>待补信息</span>
          </div>
          <div className="inbox-counter inbox-counter--work">
            <strong>{humanTasks.length}</strong>
            <span>待办人工任务</span>
          </div>
        </div>
      </header>

      {humanTasks.length > 0 ? (
        <section className="inbox-group inbox-group--work" aria-labelledby="inbox-human-work">
          <header>
            <h2 id="inbox-human-work">待办人工任务</h2>
            <span>{humanTasks.length}</span>
          </header>
          <p className="inbox-group__note">
            这些任务来自已发布的开发计划，按服务商与阶段聚合，不会混入需求问题。
          </p>
          <div className="inbox-work-groups">
            {humanWorkGroups.map((group) => (
              <HumanWorkProviderGroup
                group={group}
                busy={busy}
                busyLabel={busyLabel}
                onCompleteHuman={onCompleteHuman}
                key={group.label}
              />
            ))}
          </div>
        </section>
      ) : null}

      {openRequests.length === 0 ? (
        <section className="panel inbox-empty">
          <div className="empty-state">
            <strong>当前没有需要人工补充的信息</strong>
            <p>模型生成蓝图时提出的阻塞问题都会出现在这里；回答后会自动归档为项目事实。</p>
          </div>
        </section>
      ) : (
        <div className="inbox-groups">
          {categoryOrder.map((category) => {
            const requests = openRequests.filter((request) => request.category === category);
            if (requests.length === 0) {
              return null;
            }
            return (
              <section className="inbox-group" key={category} aria-labelledby={`inbox-${category}`}>
                <header>
                  <h2 id={`inbox-${category}`}>{categoryLabels[category]}</h2>
                  <span>{requests.length}</span>
                </header>
                <div className="inbox-list">
                  {requests.map((request) => (
                    <InboxRequestCard
                      request={request}
                      busy={busy}
                      busyLabel={busyLabel}
                      onResolve={onResolve}
                      onDismiss={onDismiss}
                      key={request.id}
                    />
                  ))}
                </div>
              </section>
            );
          })}
        </div>
      )}

      {resolvedRequests.length > 0 ? (
        <details className="inbox-archive">
          <summary>已回答与已关闭 · {resolvedRequests.length}</summary>
          <div className="inbox-archive__list">
            {resolvedRequests.map((request) => (
              <article key={request.id}>
                <div>
                  <span>{categoryLabels[request.category]}</span>
                  <span>{request.status === 'answered' ? '已回答' : '已关闭'}</span>
                </div>
                <h3>{request.question}</h3>
                <p>
                  {request.status === 'dismissed'
                    ? `关闭原因：${request.answer ?? '未填写'}`
                    : request.answer ?? '未填写答案'}
                </p>
              </article>
            ))}
          </div>
        </details>
      ) : null}

      <section className="panel facts-panel" aria-labelledby="facts-title">
        <div className="panel__head">
          <div>
            <p className="panel__eyebrow">已确认的项目事实</p>
            <h2 id="facts-title">项目事实</h2>
          </div>
          <span className="count-badge">{facts.length}</span>
        </div>
        {facts.length === 0 ? (
          <p className="editor-empty">回答 Inbox 后，已确认信息会出现在这里。</p>
        ) : (
          <ul className="facts-list">
            {facts.map((fact) => (
              <li key={fact.id}>
                <span>v{fact.version}</span>
                <div>
                  <strong>{fact.question}</strong>
                  <p>{fact.value}</p>
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>
    </section>
  );
}
