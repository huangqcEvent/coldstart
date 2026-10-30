import { useId, useState } from 'react';

import type { InfoRequest, InfoRequestCategory, ProjectFact } from '../api/types';

interface InboxViewProps {
  inbox: InfoRequest[];
  facts: ProjectFact[];
  busy: boolean;
  busyLabel: string | null;
  onResolve: (requestId: string, answer: string) => Promise<boolean>;
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

function InboxRequestCard({
  request,
  busy,
  busyLabel,
  onResolve,
}: {
  request: InfoRequest;
  busy: boolean;
  busyLabel: string | null;
  onResolve: (requestId: string, answer: string) => Promise<boolean>;
}) {
  const answerId = useId();
  const [answer, setAnswer] = useState('');
  const submitting = busyLabel === '正在提交 Inbox 回答';

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
    </article>
  );
}

export function InboxView({
  inbox,
  facts,
  busy,
  busyLabel,
  onResolve,
}: InboxViewProps) {
  const openRequests = inbox.filter((request) => request.status === 'open');
  const resolvedRequests = inbox.filter((request) => request.status !== 'open');

  return (
    <section className="content-view" aria-labelledby="inbox-title">
      <header className="content-view__header">
        <div>
          <p className="eyebrow">HUMAN INBOX / EXCEPTION ONLY</p>
          <h1 id="inbox-title">人工 Inbox</h1>
          <p className="content-view__lead">
            开发只在缺少信息、需要决策或外部授权时中断。每条回答会成为可追溯的项目事实。
          </p>
        </div>
        <div className="inbox-counter" aria-label={`${openRequests.length} 条待处理`}>
          <strong>{openRequests.length}</strong>
          <span>待处理</span>
        </div>
      </header>

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
                <p>{request.answer ?? '未填写答案'}</p>
              </article>
            ))}
          </div>
        </details>
      ) : null}

      <section className="panel facts-panel" aria-labelledby="facts-title">
        <div className="panel__head">
          <div>
            <p className="panel__eyebrow">CONFIRMED PROJECT FACTS</p>
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
