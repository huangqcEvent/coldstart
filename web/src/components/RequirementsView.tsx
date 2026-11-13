/**
 * 需求来源与 GitHub Issue 入口页面。
 *
 * 支持导入本地文档、筛选/阅读 Issue 并触发人工导入；导入后来源保持不可变，
 * 不会自动生成或发布蓝图。
 */
import { useEffect, useId, useMemo, useState } from 'react';

import * as integrationApi from '../api/integrationApi';
import * as issueApi from '../api/issueApi';
import { GitHubProjectsPanel } from './GitHubProjectsPanel';
import type {
  ArtifactMetadata,
  CommandError,
  GitHubIssueListState,
  GitHubIssueListView,
  GitHubIssueView,
  ModelConfig,
  Repository,
} from '../api/types';

interface RequirementsViewProps {
  path: string;
  sources: ArtifactMetadata[];
  busy: boolean;
  busyLabel: string | null;
  onImport: () => void;
  onGenerate: (config: ModelConfig) => void;
  onImportIssue: (repositoryKey: string, issueNumber: number) => Promise<boolean>;
}

const formatLabels: Record<ArtifactMetadata['format'], string> = {
  markdown: 'Markdown',
  text: 'TXT',
  docx: 'DOCX',
  pdf: 'PDF',
};

const issueListStateLabels: Record<GitHubIssueListState, string> = {
  open: '开放',
  closed: '已关闭',
  all: '全部',
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

function formatIssueTime(value: string) {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }
  return new Intl.DateTimeFormat('zh-CN', {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(parsed);
}

function matchesIssueQuery(issue: GitHubIssueView, query: string) {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) {
    return true;
  }
  return [
    `#${issue.number}`,
    issue.title,
    issue.labels.join(' '),
    issue.bodyPreview,
  ]
    .join('\n')
    .toLocaleLowerCase()
    .includes(needle);
}

export function RequirementsView({
  path,
  sources,
  busy,
  busyLabel,
  onImport,
  onGenerate,
  onImportIssue,
}: RequirementsViewProps) {
  const baseUrlId = useId();
  const modelId = useId();
  const apiKeyId = useId();
  const consentId = useId();
  const issueRepositoryId = useId();
  const issueStateId = useId();
  const issueSearchId = useId();
  const issueLabelId = useId();
  const issueUnimportedId = useId();
  const [baseUrl, setBaseUrl] = useState('');
  const [model, setModel] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [consent, setConsent] = useState(false);
  const [repositories, setRepositories] = useState<Repository[]>([]);
  const [repositoryKey, setRepositoryKey] = useState('');
  const [issueState, setIssueState] = useState<GitHubIssueListState>('open');
  const [issueList, setIssueList] = useState<GitHubIssueListView | null>(null);
  const [issueQuery, setIssueQuery] = useState('');
  const [issueLabel, setIssueLabel] = useState('');
  const [onlyUnimported, setOnlyUnimported] = useState(false);
  const [issueBusy, setIssueBusy] = useState(false);
  const [issueError, setIssueError] = useState<CommandError | null>(null);
  const [importingIssue, setImportingIssue] = useState<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    integrationApi
      .readIntegrationWorkspace(path)
      .then((workspace) => {
        if (cancelled) {
          return;
        }
        setRepositories(workspace.repositories);
        setRepositoryKey((current) =>
          workspace.repositories.some((repository) => repository.key === current)
            ? current
            : workspace.repositories[0]?.key ?? '',
        );
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setIssueError(normalizeCommandError(error));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [path]);

  const configReady =
    baseUrl.trim().length > 0 && model.trim().length > 0 && sources.length > 0;
  const canGenerate = configReady && consent && !busy;
  const issueActionBusy = busy || issueBusy || importingIssue !== null;
  const importedSourceReferences = useMemo(
    () => new Set(sources.map((source) => source.importSource)),
    [sources],
  );
  const issueLabels = useMemo(
    () =>
      Array.from(new Set((issueList?.issues ?? []).flatMap((issue) => issue.labels))).sort(
        (left, right) => left.localeCompare(right, 'zh-CN'),
      ),
    [issueList],
  );
  const visibleIssues = useMemo(
    () =>
      (issueList?.issues ?? []).filter((issue) => {
        if (!matchesIssueQuery(issue, issueQuery)) {
          return false;
        }
        if (issueLabel && !issue.labels.includes(issueLabel)) {
          return false;
        }
        if (onlyUnimported && importedSourceReferences.has(issue.sourceReference)) {
          return false;
        }
        return true;
      }),
    [importedSourceReferences, issueLabel, issueList, issueQuery, onlyUnimported],
  );
  const hasIssueFilters =
    issueQuery.trim().length > 0 || issueLabel.length > 0 || onlyUnimported;

  async function handleLoadIssues() {
    if (!repositoryKey || issueActionBusy) {
      return;
    }
    setIssueBusy(true);
    setIssueError(null);
    try {
      setIssueList(await issueApi.listGitHubIssues(path, repositoryKey, issueState));
    } catch (error) {
      setIssueList(null);
      setIssueError(normalizeCommandError(error));
    } finally {
      setIssueBusy(false);
    }
  }

  async function handleImportIssue(issueNumber: number) {
    if (!repositoryKey || issueActionBusy) {
      return;
    }
    setImportingIssue(issueNumber);
    setIssueError(null);
    try {
      await onImportIssue(repositoryKey, issueNumber);
    } finally {
      setImportingIssue(null);
    }
  }

  function isImported(sourceReference: string) {
    return importedSourceReferences.has(sourceReference);
  }

  function resetIssueFilters() {
    setIssueQuery('');
    setIssueLabel('');
    setOnlyUnimported(false);
  }

  return (
    <section className="content-view" aria-labelledby="requirements-title">
      <header className="content-view__header">
        <div>
          <p className="eyebrow">需求与来源</p>
          <h1 id="requirements-title">需求导入</h1>
          <p className="content-view__lead">
            原文件、抽取文本和锚点会一起固化到项目目录。模型只能引用这些真实锚点生成蓝图。
          </p>
        </div>
        <button
          className="button button--primary"
          type="button"
          onClick={onImport}
          disabled={busy}
        >
          {busyLabel === '正在导入需求文档' ? '导入中…' : '导入需求文档'}
        </button>
      </header>

      <div className="split-layout">
        <section className="panel" aria-labelledby="source-list-title">
          <div className="panel__head">
            <div>
              <p className="panel__eyebrow">文件与锚点</p>
              <h2 id="source-list-title">来源文档</h2>
            </div>
            <span className="count-badge">{sources.length}</span>
          </div>

          {sources.length === 0 ? (
            <div className="empty-state">
              <strong>还没有来源文档</strong>
              <p>支持 Markdown、TXT、DOCX、PDF，也可以从 GitHub Issue 导入。</p>
            </div>
          ) : (
            <ul className="source-list" aria-label="来源文档列表">
              {sources.map((source, index) => (
                <li className="source-item" key={source.id}>
                  <span className="source-item__index">
                    {String(index + 1).padStart(2, '0')}
                  </span>
                  <div className="source-item__body">
                    <div className="source-item__title-row">
                      <strong>{source.fileName}</strong>
                      <span className="format-badge">
                        {formatLabels[source.format]}
                      </span>
                    </div>
                    <p className="mono source-item__hash">
                      SHA-256 {source.contentHash.slice(0, 16)}…
                    </p>
                    <p className="source-item__meta">
                      {(source.byteSize / 1024).toFixed(1)} KiB · 已导入{' '}
                      {source.importedAt}
                    </p>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </section>

        <section className="panel" aria-labelledby="model-config-title">
          <div className="panel__head">
            <div>
              <p className="panel__eyebrow">模型连接</p>
              <h2 id="model-config-title">生成模型</h2>
            </div>
            <span className="status-dot">仅本次会话</span>
          </div>

          <div className="form-grid form-grid--single">
            <label className="field" htmlFor={baseUrlId}>
              <span>Base URL</span>
              <input
                id={baseUrlId}
                type="url"
                value={baseUrl}
                onChange={(event) => setBaseUrl(event.target.value)}
                placeholder="https://api.example.com/v1"
                autoComplete="off"
                disabled={busy}
              />
            </label>
            <label className="field" htmlFor={modelId}>
              <span>模型名称</span>
              <input
                id={modelId}
                type="text"
                value={model}
                onChange={(event) => setModel(event.target.value)}
                placeholder="例如：gpt-4.1-mini"
                autoComplete="off"
                disabled={busy}
              />
            </label>
            <label className="field" htmlFor={apiKeyId}>
              <span>API Key</span>
              <input
                id={apiKeyId}
                type="password"
                value={apiKey}
                onChange={(event) => setApiKey(event.target.value)}
                placeholder="仅保存在当前界面内存"
                autoComplete="off"
                disabled={busy}
              />
            </label>
          </div>

          <label className="consent-row" htmlFor={consentId}>
            <input
              id={consentId}
              type="checkbox"
              checked={consent}
              onChange={(event) => setConsent(event.target.checked)}
              disabled={busy}
            />
            <span>
              我确认将当前需求文档发送到所配置的模型服务。API Key
              不会写入项目文件、数据库、事件或日志。
            </span>
          </label>

          <p className="field__hint">
            生成前会检查来源总量不超过 80,000 字符。模型返回的每个来源锚点都会在写入草稿前核验。
          </p>

          <button
            className="button button--primary button--wide"
            type="button"
            onClick={() =>
              onGenerate({
                baseUrl: baseUrl.trim(),
                model: model.trim(),
                apiKey,
              })
            }
            disabled={!canGenerate}
          >
            {busyLabel === '正在生成蓝图草稿'
              ? '正在生成…'
              : '生成蓝图草稿'}
          </button>
          {!consent ? (
            <p className="field__hint field__hint--warning">
              勾选外部发送确认后才能生成。
            </p>
          ) : null}
        </section>
      </div>

      <section className="panel issue-intake" aria-labelledby="issue-intake-title">
        <div className="panel__head">
          <div>
            <p className="panel__eyebrow">外部需求入口</p>
            <h2 id="issue-intake-title">从 GitHub Issue 导入</h2>
            <p className="issue-intake__lead">
              只读读取开放或已关闭 Issue，人工选择后固化为本地需求来源。筛选只作用于当前快照，评论不会导入，也不会自动生成蓝图。
            </p>
          </div>
          <span className="status-dot">只读</span>
        </div>

        <div className="issue-intake__controls">
          <label className="field" htmlFor={issueRepositoryId}>
            <span>目标仓库</span>
            <select
              id={issueRepositoryId}
              value={repositoryKey}
              onChange={(event) => {
                setRepositoryKey(event.target.value);
                setIssueList(null);
                setIssueError(null);
                resetIssueFilters();
              }}
              disabled={issueActionBusy || repositories.length === 0}
            >
              {repositories.length === 0 ? (
                <option value="">尚未映射仓库</option>
              ) : null}
              {repositories.map((repository) => (
                <option key={repository.id} value={repository.key}>
                  {repository.name} · {repository.key}
                </option>
              ))}
            </select>
          </label>
          <label className="field" htmlFor={issueStateId}>
            <span>Issue 状态</span>
            <select
              id={issueStateId}
              value={issueState}
              onChange={(event) => {
                setIssueState(event.target.value as GitHubIssueListState);
                setIssueList(null);
                setIssueError(null);
                setIssueLabel('');
              }}
              disabled={issueActionBusy || repositories.length === 0}
            >
              <option value="open">开放</option>
              <option value="closed">已关闭</option>
              <option value="all">全部</option>
            </select>
          </label>
          <button
            className="button button--secondary"
            type="button"
            onClick={() => void handleLoadIssues()}
            disabled={!repositoryKey || issueActionBusy}
          >
            {issueBusy ? '正在读取…' : '读取 Issue'}
          </button>
        </div>

        <p className="field__hint">
          每个状态最多读取 50 条，按最近更新排序。关键词、标签和导入状态只过滤本次读取结果，不会继续翻页。
        </p>

        {issueError ? (
          <div className="issue-intake__error" role="alert">
            <strong>Issue 读取失败</strong>
            <span>{issueError.message}</span>
            <code>{issueError.code}</code>
          </div>
        ) : null}

        {repositories.length === 0 ? (
          <div className="empty-state">
            <strong>先映射一个 GitHub 仓库</strong>
            <p>到“仓库与能力”配置本地仓库、GitHub 远程地址和凭据后再读取 Issue。</p>
          </div>
        ) : issueList ? (
          issueList.issues.length > 0 ? (
            <>
              <div className="issue-intake__filters">
                <label className="field" htmlFor={issueSearchId}>
                  <span>关键词</span>
                  <input
                    id={issueSearchId}
                    type="search"
                    value={issueQuery}
                    placeholder="编号、标题、标签或正文预览"
                    onChange={(event) => setIssueQuery(event.target.value)}
                  />
                </label>
                <label className="field" htmlFor={issueLabelId}>
                  <span>标签</span>
                  <select
                    id={issueLabelId}
                    value={issueLabel}
                    onChange={(event) => setIssueLabel(event.target.value)}
                    disabled={issueLabels.length === 0}
                  >
                    <option value="">全部标签</option>
                    {issueLabels.map((label) => (
                      <option key={label} value={label}>
                        {label}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="issue-filter-toggle" htmlFor={issueUnimportedId}>
                  <input
                    id={issueUnimportedId}
                    type="checkbox"
                    checked={onlyUnimported}
                    onChange={(event) => setOnlyUnimported(event.target.checked)}
                  />
                  <span>仅显示未导入</span>
                </label>
                <button
                  className="button button--compact button--ghost issue-intake__reset"
                  type="button"
                  onClick={resetIssueFilters}
                  disabled={!hasIssueFilters}
                >
                  清除筛选
                </button>
              </div>
              <div className="issue-intake__summary">
                <span>
                  {issueList.repositoryLabel} · {issueListStateLabels[issueList.state]} Issue ·{' '}
                  {hasIssueFilters
                    ? `${visibleIssues.length}/${issueList.issues.length} 条匹配`
                    : `${issueList.issues.length} 条`}
                </span>
                {issueList.truncated ? <strong>列表已截断</strong> : null}
              </div>
              {visibleIssues.length > 0 ? (
                <ul
                  className="issue-list"
                  aria-label={`GitHub ${issueListStateLabels[issueList.state]} Issue 列表`}
                >
                  {visibleIssues.map((issue) => {
                    const imported = isImported(issue.sourceReference);
                    return (
                      <li className="issue-card" key={issue.id}>
                        <header className="issue-card__header">
                          <div className="issue-card__identity">
                            <span className="issue-card__number">#{issue.number}</span>
                            <h3>{issue.title}</h3>
                          </div>
                          <span className={`issue-state issue-state--${issue.state}`}>
                            {issue.state === 'open' ? '开放' : '已关闭'}
                          </span>
                        </header>
                        <p className="issue-card__meta">
                          {formatIssueTime(issue.updatedAt)} · {issue.comments} 条评论 · 评论未导入
                        </p>
                        {issue.labels.length > 0 ? (
                          <div className="issue-card__labels" aria-label="Issue 标签">
                            {issue.labels.map((label) => (
                              <span key={label}>{label}</span>
                            ))}
                          </div>
                        ) : null}
                        <p className="issue-card__body">
                          {issue.bodyPreview || '该 Issue 没有正文，请回到 GitHub 查看讨论。'}
                        </p>
                        <footer className="issue-card__footer">
                          <a href={issue.htmlUrl} target="_blank" rel="noreferrer">
                            在 GitHub 查看
                          </a>
                          <button
                            className="button button--compact button--primary"
                            type="button"
                            onClick={() => void handleImportIssue(issue.number)}
                            disabled={issueActionBusy || imported}
                          >
                            {imported
                              ? '已导入为来源'
                              : importingIssue === issue.number
                                ? '正在导入…'
                                : '导入为需求来源'}
                          </button>
                        </footer>
                      </li>
                    );
                  })}
                </ul>
              ) : (
                <div className="empty-state">
                  <strong>没有符合筛选条件的 Issue</strong>
                  <p>调整关键词或标签，也可以清除筛选后查看本次读取的全部结果。</p>
                  <button
                    className="button button--compact button--secondary"
                    type="button"
                    onClick={resetIssueFilters}
                  >
                    清除筛选
                  </button>
                </div>
              )}
            </>
          ) : (
            <div className="empty-state">
              <strong>当前没有{issueListStateLabels[issueList.state]} Issue</strong>
              <p>切换状态或稍后重试；Issue 状态变化后需要重新读取。</p>
            </div>
          )
        ) : null}
      </section>

      <GitHubProjectsPanel path={path} repositories={repositories} busy={busy} />
    </section>
  );
}
