/**
 * 需求来源与 GitHub Issue 入口页面。
 *
 * 支持导入本地文档、筛选/阅读 Issue 并触发人工导入；导入后来源保持不可变，
 * 不会自动生成或发布蓝图。
 */
import { useEffect, useId, useMemo, useRef, useState } from 'react';

import * as integrationApi from '../api/integrationApi';
import * as issueApi from '../api/issueApi';
import * as requirementsApi from '../api/requirementsApi';
import { GitHubProjectsPanel } from './GitHubProjectsPanel';
import type {
  ArtifactImage,
  ArtifactMetadata,
  CodeInvestigation,
  CodeInvestigationStatus,
  CommandError,
  DocumentAnchor,
  GitHubIssueListState,
  GitHubIssueListView,
  GitHubIssueView,
  ModelConfig,
  RequirementImagePreview,
  Repository,
} from '../api/types';

interface RequirementsViewProps {
  path: string;
  sources: ArtifactMetadata[];
  sourceAnchors: Record<string, DocumentAnchor[]>;
  sourceImages: Record<string, ArtifactImage[]>;
  codeInvestigations: CodeInvestigation[];
  busy: boolean;
  busyLabel: string | null;
  onImport: () => void;
  onGenerate: (config: ModelConfig, artifactIds: string[]) => void;
  onImportIssue: (repositoryKey: string, issueNumber: number) => Promise<boolean>;
  onCreateCodeInvestigation: (
    repositoryKey: string,
    baseRef: string,
    question: string,
  ) => Promise<boolean>;
  onConfirmCodeInvestigation: (investigationId: string) => Promise<boolean>;
  onRejectCodeInvestigation: (
    investigationId: string,
    reason: string,
  ) => Promise<boolean>;
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

const investigationStatusLabels: Record<CodeInvestigationStatus, string> = {
  requested: '等待 Codex 领取',
  claimed: 'Codex 调查中',
  submitted: '等待人工确认',
  confirmed: '已确认',
  rejected: '已驳回',
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
  sourceAnchors,
  sourceImages,
  codeInvestigations,
  busy,
  busyLabel,
  onImport,
  onGenerate,
  onImportIssue,
  onCreateCodeInvestigation,
  onConfirmCodeInvestigation,
  onRejectCodeInvestigation,
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
  const investigationRepositoryId = useId();
  const investigationBaseRefId = useId();
  const investigationQuestionId = useId();
  const [baseUrl, setBaseUrl] = useState('');
  const [model, setModel] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [consent, setConsent] = useState(false);
  const [selectedSourceIds, setSelectedSourceIds] = useState<string[]>([]);
  const previousSourceIds = useRef<Set<string>>(new Set());
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
  const [imagePreviews, setImagePreviews] = useState<
    Record<string, RequirementImagePreview>
  >({});
  const [loadingImage, setLoadingImage] = useState<string | null>(null);
  const [imageErrors, setImageErrors] = useState<Record<string, CommandError>>({});
  const [investigationRepositoryKey, setInvestigationRepositoryKey] = useState('');
  const [investigationBaseRef, setInvestigationBaseRef] = useState('');
  const [investigationQuestion, setInvestigationQuestion] = useState('');
  const [rejectionReasons, setRejectionReasons] = useState<Record<string, string>>({});

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
        const firstRepository = workspace.repositories[0];
        setInvestigationRepositoryKey(firstRepository?.key ?? '');
        setInvestigationBaseRef(firstRepository?.defaultBranch ?? 'main');
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
    baseUrl.trim().length > 0 &&
    model.trim().length > 0 &&
    selectedSourceIds.length > 0;
  const canGenerate = configReady && consent && !busy;
  const issueActionBusy = busy || issueBusy || importingIssue !== null;
  const importedSourceReferences = useMemo(
    () => new Set(sources.map((source) => source.importSource)),
    [sources],
  );

  useEffect(() => {
    const sourceIds = sources.map((source) => source.id);
    setSelectedSourceIds((current) => {
      if (current.length === 0) {
        return sourceIds;
      }
      const retained = current.filter((id) => sourceIds.includes(id));
      const added = sourceIds.filter((id) => !previousSourceIds.current.has(id));
      return Array.from(new Set([...retained, ...added]));
    });
    previousSourceIds.current = new Set(sourceIds);
  }, [sources]);

  function toggleSource(sourceId: string, selected: boolean) {
    setSelectedSourceIds((current) =>
      selected
        ? Array.from(new Set([...current, sourceId]))
        : current.filter((id) => id !== sourceId),
    );
  }

  async function handleCreateInvestigation() {
    const question = investigationQuestion.trim();
    if (!investigationRepositoryKey || question.length < 5 || busy) {
      return;
    }
    const created = await onCreateCodeInvestigation(
      investigationRepositoryKey,
      investigationBaseRef,
      question,
    );
    if (created) {
      setInvestigationQuestion('');
    }
  }

  async function handleRejectInvestigation(investigationId: string) {
    const reason = rejectionReasons[investigationId]?.trim() ?? '';
    if (!reason) {
      return;
    }
    const rejected = await onRejectCodeInvestigation(investigationId, reason);
    if (rejected) {
      setRejectionReasons((current) => {
        const next = { ...current };
        delete next[investigationId];
        return next;
      });
    }
  }
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
  const totalImages = useMemo(
    () =>
      Object.values(sourceImages).reduce(
        (total, images) => total + images.length,
        0,
      ),
    [sourceImages],
  );

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

  function anchorLabelFor(sourceId: string, image: ArtifactImage) {
    return (
      sourceAnchors[sourceId]?.find((anchor) => anchor.id === image.anchorId)
        ?.label ?? '未标注章节'
    );
  }

  async function handleViewImage(sourceId: string, image: ArtifactImage) {
    const imageKey = `${sourceId}:${image.id}`;
    if (imagePreviews[imageKey] || loadingImage === imageKey) {
      return;
    }

    setLoadingImage(imageKey);
    setImageErrors((current) => {
      const next = { ...current };
      delete next[imageKey];
      return next;
    });
    try {
      const preview = await requirementsApi.readRequirementImage(
        path,
        sourceId,
        image.id,
      );
      setImagePreviews((current) => ({ ...current, [imageKey]: preview }));
    } catch (error: unknown) {
      setImageErrors((current) => ({
        ...current,
        [imageKey]: normalizeCommandError(error),
      }));
    } finally {
      setLoadingImage((current) => (current === imageKey ? null : current));
    }
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

          {sources.length > 0 ? (
            <div className="source-change-set" aria-label="本次蓝图变更集">
              <div>
                <strong>
                  本次变更集 {selectedSourceIds.length}/{sources.length}
                </strong>
                <span>只把勾选来源发送给模型，并记录到新的蓝图版本。</span>
              </div>
              <div className="source-change-set__actions">
                <button
                  className="button button--ghost button--compact"
                  type="button"
                  onClick={() =>
                    setSelectedSourceIds(sources.map((source) => source.id))
                  }
                  disabled={busy}
                >
                  全选
                </button>
                <button
                  className="button button--ghost button--compact"
                  type="button"
                  onClick={() =>
                    setSelectedSourceIds(
                      sources.length > 0 ? [sources[sources.length - 1].id] : [],
                    )
                  }
                  disabled={busy || sources.length === 0}
                >
                  仅选最新导入
                </button>
              </div>
            </div>
          ) : null}

          {totalImages > 0 ? (
            <div className="source-images-note">
              <span className="source-images-note__count">{totalImages} 张图片</span>
              <p>
                原图不参与自动需求分析，仅供人工确认和 Codex 按任务权限按需查看。
              </p>
            </div>
          ) : null}

          {sources.length === 0 ? (
            <div className="empty-state">
              <strong>还没有来源文档</strong>
              <p>支持 Markdown、TXT、DOCX、PDF，也可以从 GitHub Issue 导入。</p>
            </div>
          ) : (
            <ul className="source-list" aria-label="来源文档列表">
              {sources.map((source, index) => {
                const images = sourceImages[source.id] ?? [];
                return (
                  <li className="source-item" key={source.id}>
                    <span className="source-item__index">
                      {String(index + 1).padStart(2, '0')}
                    </span>
                    <div className="source-item__body">
                      <label className="source-select">
                        <input
                          type="checkbox"
                          checked={selectedSourceIds.includes(source.id)}
                          onChange={(event) =>
                            toggleSource(source.id, event.target.checked)
                          }
                          disabled={busy}
                        />
                        <span>纳入本次蓝图变更集</span>
                      </label>
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
                      {images.length > 0 ? (
                        <section
                          className="source-images"
                          aria-label={`${source.fileName} 的来源图片`}
                        >
                          <div className="source-images__head">
                            <strong>图片 {images.length} 张</strong>
                            <span>按需预览，不进入模型上下文</span>
                          </div>
                          <ul className="source-images__list">
                            {images.map((image) => {
                              const imageKey = `${source.id}:${image.id}`;
                              const label = anchorLabelFor(source.id, image);
                              const preview = imagePreviews[imageKey];
                              const error = imageErrors[imageKey];
                              const isLoading = loadingImage === imageKey;
                              return (
                                <li className="source-image" key={imageKey}>
                                  <div className="source-image__meta">
                                    <strong>{image.fileName}</strong>
                                    <span>章节：{label}</span>
                                    <span>
                                      {image.mimeType} ·{' '}
                                      {(image.byteSize / 1024).toFixed(1)} KiB
                                    </span>
                                  </div>
                                  <button
                                    className="button button--compact button--ghost source-image__button"
                                    type="button"
                                    aria-label={`查看原图 ${image.fileName}（${label}）`}
                                    onClick={() =>
                                      void handleViewImage(source.id, image)
                                    }
                                    disabled={isLoading || preview !== undefined}
                                  >
                                    {isLoading
                                      ? '读取中…'
                                      : preview
                                        ? '已加载'
                                        : '查看原图'}
                                  </button>
                                  {error ? (
                                    <p className="source-image__error" role="alert">
                                      {error.message}（{error.code}）
                                    </p>
                                  ) : null}
                                  {preview ? (
                                    <figure className="source-image__preview">
                                      <img
                                        src={`data:${preview.image.mimeType};base64,${preview.dataBase64}`}
                                        alt={`${label} 来源图片：${preview.image.fileName}`}
                                      />
                                      <figcaption>
                                        {preview.sourceFileName} · {preview.anchorLabel}{' '}
                                        · 仅本地预览
                                      </figcaption>
                                    </figure>
                                  ) : null}
                                </li>
                              );
                            })}
                          </ul>
                        </section>
                      ) : null}
                    </div>
                  </li>
                );
              })}
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
            生成前会检查本次变更集总量不超过 80,000 字符；历史文档可以保留在项目中但不参与本轮分析。模型返回的每个来源锚点都会在写入草稿前核验。
          </p>

          <button
            className="button button--primary button--wide"
            type="button"
            onClick={() =>
              onGenerate(
                {
                  baseUrl: baseUrl.trim(),
                  model: model.trim(),
                  apiKey,
                },
                selectedSourceIds,
              )
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

      <section
        className="panel code-investigation"
        aria-labelledby="code-investigation-title"
      >
        <div className="panel__head">
          <div>
            <p className="panel__eyebrow">历史项目接入</p>
            <h2 id="code-investigation-title">只读代码调查</h2>
            <p className="issue-intake__lead">
              为本次需求创建调查问题，由 Codex 通过 MCP 主动领取并读取已映射仓库。
              它只能上报路径、行号和风险；人工确认前不会进入蓝图上下文。
            </p>
          </div>
          <span className="status-dot">人工确认</span>
        </div>

        <div className="code-investigation__form">
          <label className="field" htmlFor={investigationRepositoryId}>
            <span>调查仓库</span>
            <select
              id={investigationRepositoryId}
              value={investigationRepositoryKey}
              onChange={(event) => {
                const key = event.target.value;
                const repository = repositories.find((item) => item.key === key);
                setInvestigationRepositoryKey(key);
                setInvestigationBaseRef(repository?.defaultBranch ?? 'main');
              }}
              disabled={busy || repositories.length === 0}
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
          <label className="field" htmlFor={investigationBaseRefId}>
            <span>调查基线分支</span>
            <input
              id={investigationBaseRefId}
              type="text"
              value={investigationBaseRef}
              onChange={(event) => setInvestigationBaseRef(event.target.value)}
              placeholder="main"
              autoComplete="off"
              disabled={busy || repositories.length === 0}
            />
          </label>
          <label
            className="field code-investigation__question"
            htmlFor={investigationQuestionId}
          >
            <span>需要 Codex 查清什么</span>
            <textarea
              id={investigationQuestionId}
              value={investigationQuestion}
              onChange={(event) => setInvestigationQuestion(event.target.value)}
              placeholder="例如：现有订单创建入口、库存事务边界和可复用的鉴权中间件在哪里？新增优惠能力可能影响哪些模块？"
              rows={4}
              disabled={busy || repositories.length === 0}
            />
          </label>
        </div>

        <div className="code-investigation__create">
          <p className="field__hint">
            调查不下载整库、不保存源码正文，也不会把本地路径或 Secret 上报给桌面端。
            报告提交后必须由你确认，才可作为下一次蓝图生成的参考证据。
          </p>
          <button
            className="button button--secondary"
            type="button"
            onClick={() => void handleCreateInvestigation()}
            disabled={
              busy ||
              !investigationRepositoryKey ||
              investigationQuestion.trim().length < 5
            }
          >
            {busyLabel === '正在创建历史代码调查'
              ? '创建中…'
              : '创建调查请求'}
          </button>
        </div>

        {repositories.length === 0 ? (
          <div className="empty-state">
            <strong>先映射历史项目仓库</strong>
            <p>到“仓库与能力”配置本地 Git 工作区后，才能创建只读调查。</p>
          </div>
        ) : codeInvestigations.length === 0 ? (
          <div className="empty-state">
            <strong>还没有历史代码调查</strong>
            <p>创建问题后，Codex 会通过 MCP 主动领取；桌面端不会自行启动 Agent。</p>
          </div>
        ) : (
          <ul className="code-investigation__list">
            {codeInvestigations.map((investigation) => {
              const submitted = investigation.status === 'submitted';
              const canShowReport =
                investigation.status === 'submitted' ||
                investigation.status === 'confirmed';
              return (
                <li
                  className={`code-investigation-card code-investigation-card--${investigation.status}`}
                  key={investigation.id}
                >
                  <header className="code-investigation-card__head">
                    <div>
                      <span
                        className={`code-investigation-status code-investigation-status--${investigation.status}`}
                      >
                        {investigationStatusLabels[investigation.status]}
                      </span>
                      <code>{investigation.repositoryKey}</code>
                      <span>基线 {investigation.baseRef}</span>
                    </div>
                    <time dateTime={investigation.createdAt}>
                      {formatIssueTime(investigation.createdAt)}
                    </time>
                  </header>
                  <h3>{investigation.question}</h3>
                  <dl className="code-investigation-card__meta">
                    {investigation.agentProfile ? (
                      <div>
                        <dt>领取者</dt>
                        <dd>{investigation.agentProfile}</dd>
                      </div>
                    ) : null}
                    {investigation.leaseExpiresAt &&
                    investigation.status === 'claimed' ? (
                      <div>
                        <dt>领取有效期</dt>
                        <dd>{formatIssueTime(investigation.leaseExpiresAt)}</dd>
                      </div>
                    ) : null}
                    {investigation.commitSha ? (
                      <div>
                        <dt>Commit</dt>
                        <dd className="mono">{investigation.commitSha.slice(0, 12)}</dd>
                      </div>
                    ) : null}
                  </dl>
                  {investigation.summary ? (
                    <p className="code-investigation-card__summary">
                      {investigation.summary}
                    </p>
                  ) : null}
                  {canShowReport && investigation.findings.length > 0 ? (
                    <ol className="code-findings">
                      {investigation.findings.map((finding, index) => (
                        <li key={`${investigation.id}:${index}`}>
                          <div className="code-findings__location">
                            <code>
                              {finding.path}:{finding.lineStart}-{finding.lineEnd}
                            </code>
                            {finding.symbol ? <span>{finding.symbol}</span> : null}
                          </div>
                          <p>{finding.summary}</p>
                          <p className="code-findings__risk">风险：{finding.risk}</p>
                          {finding.suggestedChangePaths.length > 0 ? (
                            <p className="code-findings__paths">
                              建议修改范围：{finding.suggestedChangePaths.join('、')}
                            </p>
                          ) : null}
                        </li>
                      ))}
                    </ol>
                  ) : null}
                  {canShowReport && investigation.limitations.length > 0 ? (
                    <details className="code-investigation-card__limits">
                      <summary>调查限制 · {investigation.limitations.length}</summary>
                      <ul>
                        {investigation.limitations.map((limitation) => (
                          <li key={limitation}>{limitation}</li>
                        ))}
                      </ul>
                    </details>
                  ) : null}
                  {investigation.status === 'rejected' ? (
                    <p className="code-investigation-card__rejected" role="status">
                      驳回原因：{investigation.rejectionReason ?? '未填写'}
                    </p>
                  ) : null}
                  {submitted ? (
                    <div className="code-investigation-card__review">
                      <label className="field">
                        <span>驳回原因</span>
                        <textarea
                          value={rejectionReasons[investigation.id] ?? ''}
                          onChange={(event) =>
                            setRejectionReasons((current) => ({
                              ...current,
                              [investigation.id]: event.target.value,
                            }))
                          }
                          placeholder="证据不足时填写需要补充的调查范围"
                          aria-label={`驳回调查：${investigation.question}`}
                          rows={2}
                          disabled={busy}
                        />
                      </label>
                      <div className="code-investigation-card__actions">
                        <button
                          className="button button--compact button--primary"
                          type="button"
                          onClick={() =>
                            void onConfirmCodeInvestigation(investigation.id)
                          }
                          disabled={busy}
                        >
                          {busyLabel === '正在确认历史代码调查'
                            ? '确认中…'
                            : '确认证据'}
                        </button>
                        <button
                          className="button button--compact button--ghost code-investigation-card__reject"
                          type="button"
                          onClick={() => void handleRejectInvestigation(investigation.id)}
                          disabled={
                            busy || !(rejectionReasons[investigation.id] ?? '').trim()
                          }
                        >
                          {busyLabel === '正在驳回历史代码调查'
                            ? '驳回中…'
                            : '驳回报告'}
                        </button>
                      </div>
                    </div>
                  ) : null}
                </li>
              );
            })}
          </ul>
        )}
      </section>

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
