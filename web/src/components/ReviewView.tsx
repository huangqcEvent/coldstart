import { useEffect, useState } from 'react';

import * as integrationApi from '../api/integrationApi';
import * as reviewApi from '../api/reviewApi';
import type {
  CommandError,
  CredentialMetadata,
  ReviewConfiguration,
  ReviewHistoryView,
  ReviewScope,
  ReviewSeverity,
  ReviewStandardContent,
  ReviewStandardView,
  ReviewWorkspace,
} from '../api/types';
import { ErrorBanner } from './ErrorBanner';
import { ReviewFindingList } from './ReviewFindingList';
import {
  ReviewStandardEditor,
  type ReviewStandardEditorMode,
} from './ReviewStandardEditor';

interface ReviewViewProps {
  path: string;
  onPlanChanged?: () => Promise<void>;
}

interface ReviewConfigForm {
  enabled: boolean;
  baseUrl: string;
  model: string;
  credentialId: string;
  blockingSeverities: ReviewSeverity[];
  consentConfirmed: boolean;
}

interface ReviewEditorState {
  mode: ReviewStandardEditorMode;
  content: ReviewStandardContent | null;
}

const repositoryLabels: Record<string, string> = {
  server: 'server / 服务端',
  admin: 'admin / 管理后台',
  web: 'web / Web 前台',
  app: 'app / 移动 App',
  mini_program: 'mini_program / 小程序',
};

const severityOptions: Array<{ value: ReviewSeverity; label: string }> = [
  { value: 'critical', label: '严重 / critical' },
  { value: 'high', label: '高 / high' },
  { value: 'medium', label: '中 / medium' },
  { value: 'low', label: '低 / low' },
];

const statusLabels = {
  not_required: '无需评审',
  running: '评审中',
  passed: '已通过',
  blocked: '已阻断',
  failed: '评审失败',
  waived: '人工豁免',
  interrupted: '已中断',
} as const;

function normalizeCommandError(error: unknown): CommandError {
  if (typeof error === 'object' && error !== null) {
    const candidate = error as Record<string, unknown>;
    if (typeof candidate.code === 'string' && typeof candidate.message === 'string') {
      return { code: candidate.code, message: candidate.message };
    }
  }
  return { code: 'unknown', message: String(error) };
}

function configForm(configuration: ReviewConfiguration | null): ReviewConfigForm {
  return configuration
    ? {
        enabled: configuration.enabled,
        baseUrl: configuration.baseUrl,
        model: configuration.model,
        credentialId: configuration.credentialId ?? '',
        blockingSeverities: [...configuration.blockingSeverities],
        consentConfirmed: false,
      }
    : {
        enabled: false,
        baseUrl: 'https://api.openai.com/v1',
        model: '',
        credentialId: '',
        blockingSeverities: ['critical', 'high'],
        consentConfirmed: false,
      };
}

function formatTimestamp(value: string) {
  return new Date(value).toLocaleString('zh-CN', { hour12: false });
}

function scopeLabel(scope: ReviewScope) {
  return scope.kind === 'project'
    ? '全项目'
    : `${repositoryLabels[scope.repositoryKey] ?? scope.repositoryKey}`;
}

function standardCharacters(standards: ReviewStandardView[]) {
  return standards
    .filter((standard) => standard.enabled)
    .reduce((total, standard) => total + standard.characterCount, 0);
}

export function ReviewView({ path, onPlanChanged }: ReviewViewProps) {
  const [workspace, setWorkspace] = useState<ReviewWorkspace | null>(null);
  const [credentials, setCredentials] = useState<CredentialMetadata[]>([]);
  const [form, setForm] = useState<ReviewConfigForm>(configForm(null));
  const [loading, setLoading] = useState(true);
  const [busyLabel, setBusyLabel] = useState<string | null>(null);
  const [error, setError] = useState<CommandError | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [editor, setEditor] = useState<ReviewEditorState | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<ReviewStandardView | null>(null);
  const [waiverTarget, setWaiverTarget] = useState<ReviewHistoryView | null>(null);
  const [waiverReason, setWaiverReason] = useState('');
  const [waiverStep, setWaiverStep] = useState<'reason' | 'confirm'>('reason');

  async function loadWorkspace(resetConfiguration: boolean) {
    const [reviewWorkspace, integration] = await Promise.all([
      reviewApi.readReviewWorkspace(path),
      integrationApi.readIntegrationWorkspace(path),
    ]);
    setWorkspace(reviewWorkspace);
    setCredentials(integration.credentials);
    if (resetConfiguration) {
      setForm(configForm(reviewWorkspace.configuration));
    }
  }

  useEffect(() => {
    let active = true;
    setLoading(true);
    setError(null);
    void Promise.all([
      reviewApi.readReviewWorkspace(path),
      integrationApi.readIntegrationWorkspace(path),
    ])
      .then(([reviewWorkspace, integration]) => {
        if (!active) {
          return;
        }
        setWorkspace(reviewWorkspace);
        setCredentials(integration.credentials);
        setForm(configForm(reviewWorkspace.configuration));
      })
      .catch((loadError: unknown) => {
        if (active) {
          setError(normalizeCommandError(loadError));
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
  }, [path]);

  function toggleSeverity(severity: ReviewSeverity) {
    setForm((current) => ({
      ...current,
      blockingSeverities: current.blockingSeverities.includes(severity)
        ? current.blockingSeverities.filter((value) => value !== severity)
        : severityOptions
            .map((option) => option.value)
            .filter(
              (value) =>
                value === severity || current.blockingSeverities.includes(value),
            ),
    }));
  }

  async function handleSaveConfiguration() {
    if (busyLabel) {
      return;
    }
    setBusyLabel('正在保存 AI Review 配置');
    setError(null);
    setNotice(null);
    try {
      await reviewApi.saveReviewConfiguration(path, {
        enabled: form.enabled,
        baseUrl: form.baseUrl.trim(),
        model: form.model.trim(),
        credentialId: form.credentialId || null,
        blockingSeverities: form.blockingSeverities,
        consentConfirmed: form.consentConfirmed,
      });
      await loadWorkspace(true);
      setNotice('AI Review 配置已保存；新配置只影响下一次领取。');
    } catch (saveError) {
      setError(normalizeCommandError(saveError));
    } finally {
      setBusyLabel(null);
    }
  }

  async function importStandard(
    input: Parameters<typeof reviewApi.importReviewStandard>[1],
  ) {
    setBusyLabel('正在导入评审规范');
    setError(null);
    try {
      await reviewApi.importReviewStandard(path, input);
      await loadWorkspace(false);
      setEditor(null);
      setNotice('评审规范已导入并启用。');
      return true;
    } catch (importError) {
      setError(normalizeCommandError(importError));
      return false;
    } finally {
      setBusyLabel(null);
    }
  }

  async function createStandard(
    input: Parameters<typeof reviewApi.createReviewStandard>[1],
  ) {
    setBusyLabel('正在创建评审规范');
    setError(null);
    try {
      await reviewApi.createReviewStandard(path, input);
      await loadWorkspace(false);
      setEditor(null);
      setNotice('评审规范已创建并启用。');
      return true;
    } catch (createError) {
      setError(normalizeCommandError(createError));
      return false;
    } finally {
      setBusyLabel(null);
    }
  }

  async function reviseStandard(
    standardId: string,
    input: Parameters<typeof reviewApi.reviseReviewStandard>[2],
  ) {
    setBusyLabel('正在保存规范修订');
    setError(null);
    try {
      await reviewApi.reviseReviewStandard(path, standardId, input);
      await loadWorkspace(false);
      setEditor(null);
      setNotice('规范新修订已保存；已领取任务继续使用原快照。');
      return true;
    } catch (reviseError) {
      setError(normalizeCommandError(reviseError));
      return false;
    } finally {
      setBusyLabel(null);
    }
  }

  async function openStandard(
    standard: ReviewStandardView,
    mode: 'view' | 'revise',
  ) {
    if (busyLabel) {
      return;
    }
    setBusyLabel('正在读取规范原文');
    setError(null);
    try {
      const content = await reviewApi.readReviewStandard(path, standard.id);
      setEditor({ mode, content });
    } catch (readError) {
      setError(normalizeCommandError(readError));
    } finally {
      setBusyLabel(null);
    }
  }

  async function setStandardEnabled(standard: ReviewStandardView, enabled: boolean) {
    if (busyLabel) {
      return;
    }
    setBusyLabel(enabled ? '正在启用评审规范' : '正在停用评审规范');
    setError(null);
    setNotice(null);
    try {
      setWorkspace(
        await reviewApi.setReviewStandardEnabled(path, standard.id, enabled),
      );
    } catch (updateError) {
      setError(normalizeCommandError(updateError));
    } finally {
      setBusyLabel(null);
    }
  }

  async function moveStandard(index: number, offset: -1 | 1) {
    if (!workspace || busyLabel) {
      return;
    }
    const target = index + offset;
    if (target < 0 || target >= workspace.standards.length) {
      return;
    }
    const ids = workspace.standards.map((standard) => standard.id);
    [ids[index], ids[target]] = [ids[target], ids[index]];
    setBusyLabel('正在调整规范顺序');
    setError(null);
    setNotice(null);
    try {
      setWorkspace(await reviewApi.reorderReviewStandards(path, ids));
    } catch (reorderError) {
      setError(normalizeCommandError(reorderError));
    } finally {
      setBusyLabel(null);
    }
  }

  async function confirmDelete() {
    if (!deleteTarget || busyLabel) {
      return;
    }
    setBusyLabel('正在删除评审规范');
    setError(null);
    setNotice(null);
    try {
      setWorkspace(await reviewApi.deleteReviewStandard(path, deleteTarget.id));
      setDeleteTarget(null);
      setNotice('规范已软删除；历史评审与已领取快照不受影响。');
    } catch (deleteError) {
      setError(normalizeCommandError(deleteError));
    } finally {
      setBusyLabel(null);
    }
  }

  function openWaiver(item: ReviewHistoryView) {
    if (item.reviewStatus !== 'blocked') {
      return;
    }
    setWaiverTarget(item);
    setWaiverReason('');
    setWaiverStep('reason');
  }

  async function confirmWaiver() {
    if (!waiverTarget || busyLabel) {
      return;
    }
    const reason = waiverReason.trim();
    if (!reason) {
      return;
    }
    setBusyLabel('正在记录人工豁免');
    setError(null);
    setNotice(null);
    try {
      await reviewApi.waiveWorkReview(
        path,
        waiverTarget.workItemId,
        waiverTarget.reportId,
        reason,
      );
      setWaiverTarget(null);
      setWaiverReason('');
      await loadWorkspace(false);
      if (onPlanChanged) {
        await onPlanChanged();
      }
      setNotice('人工豁免已记录；任务完成并进入推送流程。');
    } catch (waiverError) {
      setError(normalizeCommandError(waiverError));
    } finally {
      setBusyLabel(null);
    }
  }

  const busy = busyLabel !== null;
  const enabledStandards =
    workspace?.standards.filter((standard) => standard.enabled) ?? [];

  return (
    <section className="content-view review-view" aria-labelledby="review-view-title">
      <header className="content-view__header">
        <div>
          <p className="eyebrow">代码评审 · 基于真实 diff</p>
          <h1 id="review-view-title">AI 评审</h1>
          <p className="content-view__lead">
            本地验证通过后，ColdStart 从真实 Git commit 提取 diff，对照任务验收标准和启用的代码规范做最终把关。Agent 的自报结果不能替代评审。
          </p>
        </div>
        <div className="revision-summary">
          <span className="integration-status integration-status--verified">完成门禁</span>
          <code>验证 → 评审 → 推送</code>
        </div>
      </header>

      {error ? <ErrorBanner error={error} onDismiss={() => setError(null)} /> : null}
      {notice ? (
        <p className="review-notice" role="status">
          {notice}
        </p>
      ) : null}
      {busyLabel ? (
        <p className="integration-busy" role="status">
          {busyLabel}…
        </p>
      ) : null}

      {loading ? (
        <section className="panel loading-view" role="status">
          <p className="eyebrow">正在加载评审配置</p>
          <h2>正在读取评审配置、规范库和历史</h2>
        </section>
      ) : null}

      {!loading && workspace ? (
        <>
          <section className="panel review-config-panel" aria-labelledby="review-config-title">
            <div className="panel__head">
              <div>
                <p className="panel__eyebrow">模型门禁 · 失败关闭</p>
                <h2 id="review-config-title">模型与阻断策略</h2>
              </div>
              <span
                className={`integration-status ${
                  workspace.configuration?.enabled
                    ? 'integration-status--verified'
                    : 'integration-status--disabled'
                }`}
              >
                {workspace.configuration?.enabled ? '已启用' : '未启用'}
              </span>
            </div>

            <div className="review-privacy-warning">
              <strong>发送边界</strong>
              <p>
                本次 diff、任务验收标准和启用规范会发送到该模型服务。ColdStart 不发送 API Key、Secret、完整环境变量或 diff 之外的文件。
              </p>
            </div>

            <div className="review-config-grid">
              <label className="review-config-toggle">
                <input
                  type="checkbox"
                  checked={form.enabled}
                  onChange={(event) =>
                    setForm((current) => ({ ...current, enabled: event.target.checked }))
                  }
                  disabled={busy}
                  aria-label="启用 AI Review"
                />
                <span>
                  <strong>启用 AI Review</strong>
                  <small>关闭时新领取的任务不执行模型评审。</small>
                </span>
              </label>

              <label className="field">
                <span>OpenAI 兼容 Base URL</span>
                <input
                  value={form.baseUrl}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      baseUrl: event.target.value,
                      consentConfirmed: false,
                    }))
                  }
                  placeholder="https://api.openai.com/v1"
                  aria-label="Review 模型 Base URL"
                  disabled={busy}
                />
              </label>

              <label className="field">
                <span>模型名称</span>
                <input
                  value={form.model}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      model: event.target.value,
                      consentConfirmed: false,
                    }))
                  }
                  placeholder="gpt-5.6-sol"
                  aria-label="Review 模型名称"
                  disabled={busy}
                />
              </label>

              <label className="field">
                <span>模型凭据（Windows 凭据库引用）</span>
                <select
                  value={form.credentialId}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      credentialId: event.target.value,
                      consentConfirmed: false,
                    }))
                  }
                  aria-label="Review 模型凭据"
                  disabled={busy}
                >
                  <option value="">不绑定凭据（适用于本地模型）</option>
                  {credentials.map((credential) => (
                    <option value={credential.id} key={credential.id}>
                      {credential.name} · {credential.provider} · {credential.environment}
                    </option>
                  ))}
                </select>
              </label>
            </div>

            <fieldset className="review-severity-options">
              <legend>阻断完成的问题级别</legend>
              {severityOptions.map((option) => (
                <label key={option.value}>
                  <input
                    type="checkbox"
                    checked={form.blockingSeverities.includes(option.value)}
                    onChange={() => toggleSeverity(option.value)}
                    disabled={busy}
                    aria-label={`阻断级别 ${option.value}`}
                  />
                  <span>{option.label}</span>
                </label>
              ))}
              <p className="field__hint">
                默认阻断 critical 与 high。留空时也会回退到这两级；评审失败、超时或输出非法始终失败关闭。
              </p>
            </fieldset>

            <label className="review-consent">
              <input
                type="checkbox"
                checked={form.consentConfirmed}
                onChange={(event) =>
                  setForm((current) => ({
                    ...current,
                    consentConfirmed: event.target.checked,
                  }))
                }
                disabled={busy || !form.enabled}
                aria-label="确认 AI Review 隐私边界"
              />
              <span>
                我确认将本次 diff、任务验收标准和启用规范发送到上述模型服务；模型、Base URL 或凭据变化后需要重新确认。
              </span>
            </label>

            <div className="view-actions integration-form__actions">
              <button
                className="button button--primary"
                type="button"
                onClick={() => void handleSaveConfiguration()}
                disabled={busy}
              >
                保存 Review 配置
              </button>
              {workspace.configuration?.credentialId ? (
                <span className="field__hint">当前凭据引用已保存，Secret 不会回显。</span>
              ) : null}
            </div>
          </section>

          <section className="panel review-standards-panel" aria-labelledby="review-standards-title">
            <div className="panel__head">
              <div>
                <p className="panel__eyebrow">代码规范 · 版本化快照</p>
                <h2 id="review-standards-title">代码规范库</h2>
              </div>
              <span className="count-badge">{workspace.standards.length} / 20</span>
            </div>
            <p className="content-view__lead review-panel-lead">
              规范可在评审判定中作为对照依据。领取任务时会原子固化启用版本，之后修改配置或规范只影响下一次领取。
            </p>
            <div className="review-standard-summary">
              <span>{enabledStandards.length} 个启用规范</span>
              <span>{standardCharacters(workspace.standards).toLocaleString()} / 60,000 字符</span>
              <span>顺序即发送顺序</span>
            </div>

            <div className="view-actions review-standard-actions">
              <button
                className="button button--secondary"
                type="button"
                onClick={() => setEditor({ mode: 'upload', content: null })}
                disabled={busy || workspace.standards.length >= 20}
              >
                上传规范
              </button>
              <button
                className="button button--secondary"
                type="button"
                onClick={() => setEditor({ mode: 'manual', content: null })}
                disabled={busy || workspace.standards.length >= 20}
              >
                手工新建
              </button>
            </div>

            {workspace.standards.length === 0 ? (
              <div className="empty-state">
                <strong>规范库还是空的</strong>
                <p>可以上传团队现有规范，也可以直接维护 Markdown。没有规范时，AI 评审仍会使用任务验收标准。</p>
              </div>
            ) : (
              <div className="review-standard-list">
                {workspace.standards.map((standard, index) => (
                  <article
                    className={`review-standard${
                      standard.enabled ? '' : ' review-standard--disabled'
                    }`}
                    key={standard.id}
                  >
                    <header>
                      <div>
                        <strong>{standard.name}</strong>
                        <div className="review-standard__badges">
                          <span>{scopeLabel(standard.scope)}</span>
                          <span>v{standard.revisionNumber}</span>
                          <span>{standard.enabled ? '启用' : '停用'}</span>
                          <span>{standard.source === 'upload' ? '文件导入' : '手工维护'}</span>
                        </div>
                      </div>
                      <div className="review-standard__order">
                        <button
                          className="button button--quiet button--compact"
                          type="button"
                          onClick={() => void moveStandard(index, -1)}
                          disabled={busy || index === 0}
                          aria-label={`上移规范 ${standard.name}`}
                        >
                          上移
                        </button>
                        <button
                          className="button button--quiet button--compact"
                          type="button"
                          onClick={() => void moveStandard(index, 1)}
                          disabled={busy || index === workspace.standards.length - 1}
                          aria-label={`下移规范 ${standard.name}`}
                        >
                          下移
                        </button>
                      </div>
                    </header>
                    <p className="review-standard__meta">
                      {standard.fileName} · {standard.characterCount.toLocaleString()} 字符 · 哈希{' '}
                      <code>{standard.contentHash.slice(0, 12)}…</code> · 更新{' '}
                      {formatTimestamp(standard.updatedAt)}
                    </p>
                    <div className="review-standard__actions">
                      <button
                        className="button button--quiet button--compact"
                        type="button"
                        onClick={() => void openStandard(standard, 'view')}
                        disabled={busy}
                      >
                        查看原文
                      </button>
                      <button
                        className="button button--quiet button--compact"
                        type="button"
                        onClick={() => void openStandard(standard, 'revise')}
                        disabled={busy}
                      >
                        新建修订
                      </button>
                      <button
                        className="button button--quiet button--compact"
                        type="button"
                        onClick={() => void setStandardEnabled(standard, !standard.enabled)}
                        disabled={busy || (!standard.enabled && enabledStandards.length >= 20)}
                      >
                        {standard.enabled ? '停用' : '启用'}
                      </button>
                      <button
                        className="button button--quiet button--compact button--danger"
                        type="button"
                        onClick={() => setDeleteTarget(standard)}
                        disabled={busy}
                      >
                        软删除
                      </button>
                    </div>
                  </article>
                ))}
              </div>
            )}
          </section>

          <section className="panel review-history-panel" aria-labelledby="review-history-title">
            <div className="panel__head">
              <div>
                <p className="panel__eyebrow">最近评审记录</p>
                <h2 id="review-history-title">评审历史</h2>
              </div>
              <span className="count-badge">{workspace.recentReviews.length}</span>
            </div>

            {workspace.recentReviews.length === 0 ? (
              <div className="empty-state">
                <strong>还没有评审记录</strong>
                <p>配置启用后，下一次 Codex 提交且本地验证通过时会在此生成评审结果。</p>
              </div>
            ) : (
              <div className="review-history-list">
                {workspace.recentReviews.map((item) => (
                  <article className="review-history" key={item.reportId}>
                    <header>
                      <div>
                        <span
                          className={`review-status review-status--${item.reviewStatus}`}
                        >
                          {statusLabels[item.reviewStatus]}
                        </span>
                        <code>{item.taskKey}</code>
                      </div>
                      <time dateTime={item.createdAt}>{formatTimestamp(item.createdAt)}</time>
                    </header>

                    {item.review ? (
                      <>
                        <p className="review-history__summary">{item.review.summary}</p>
                        <div className="review-history__meta">
                          <span>模型 {item.review.model}</span>
                          <span>
                            耗时 {item.review.durationMs?.toLocaleString() ?? '未知'} ms
                          </span>
                          <span>{item.review.findings.length} 个问题</span>
                        </div>
                        {item.review.failure ? (
                          <p className="review-history__failure">
                            <strong>{item.review.failure.code}</strong>
                            {item.review.failure.message}
                          </p>
                        ) : null}
                        <ReviewFindingList findings={item.review.findings} />
                      </>
                    ) : (
                      <p className="content-view__lead review-panel-lead">
                        {item.reviewStatus === 'interrupted'
                          ? '租约到期或进程中断，本次评审没有完成。重新领取并提交后会生成新记录。'
                          : '评审结果尚未写入。'}
                      </p>
                    )}

                    {item.waiver ? (
                      <div className="review-waiver-record">
                        <strong>已由 {item.waiver.waivedBy} 人工豁免</strong>
                        <p>{item.waiver.reason}</p>
                        <small>{formatTimestamp(item.waiver.waivedAt)}</small>
                      </div>
                    ) : null}

                    {item.reviewStatus === 'blocked' && !item.waiver ? (
                      <button
                        className="button button--secondary button--compact"
                        type="button"
                        onClick={() => openWaiver(item)}
                        disabled={busy}
                      >
                        人工豁免…
                      </button>
                    ) : null}
                  </article>
                ))}
              </div>
            )}
          </section>
        </>
      ) : null}

      {editor ? (
        <ReviewStandardEditor
          mode={editor.mode}
          content={editor.content}
          busy={busy}
          onClose={() => setEditor(null)}
          onImport={importStandard}
          onCreate={createStandard}
          onRevise={reviseStandard}
        />
      ) : null}

      {deleteTarget ? (
        <div className="dialog-backdrop" role="presentation">
          <section
            className="dialog review-confirm-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="delete-standard-title"
          >
            <div className="dialog__head">
              <div>
                <p className="eyebrow">软删除 · 保留审计</p>
                <h2 id="delete-standard-title">确认软删除规范</h2>
              </div>
            </div>
            <p className="content-view__lead">
              规范 <strong>{deleteTarget.name}</strong> 将不再参与后续领取，但历史修订和已有任务快照会保留。
            </p>
            <div className="dialog__actions">
              <button
                className="button button--ghost"
                type="button"
                onClick={() => setDeleteTarget(null)}
                disabled={busy}
              >
                取消
              </button>
              <button
                className="button button--danger"
                type="button"
                onClick={() => void confirmDelete()}
                disabled={busy}
              >
                确认软删除
              </button>
            </div>
          </section>
        </div>
      ) : null}

      {waiverTarget ? (
        <div className="dialog-backdrop" role="presentation">
          <section
            className="dialog review-confirm-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="review-waiver-title"
          >
            <div className="dialog__head">
              <div>
                <p className="eyebrow">人工豁免 · 仅桌面端</p>
                <h2 id="review-waiver-title">人工豁免 AI Review</h2>
              </div>
            </div>
            <p className="content-view__lead">
              任务 <code>{waiverTarget.taskKey}</code> 的评审状态为阻断。豁免只适用于误报或已确认可接受的风险；记录理由后，ColdStart 会完成任务并尝试推送普通任务分支。
            </p>
            {waiverStep === 'reason' ? (
              <>
                <label className="field">
                  <span>豁免理由（必填，会进入审计记录）</span>
                  <textarea
                    value={waiverReason}
                    onChange={(event) => setWaiverReason(event.target.value)}
                    rows={5}
                    maxLength={2000}
                    aria-label="人工豁免理由"
                    disabled={busy}
                  />
                </label>
                <div className="dialog__actions">
                  <button
                    className="button button--ghost"
                    type="button"
                    onClick={() => setWaiverTarget(null)}
                    disabled={busy}
                  >
                    取消
                  </button>
                  <button
                    className="button button--secondary"
                    type="button"
                    onClick={() => setWaiverStep('confirm')}
                    disabled={busy || !waiverReason.trim()}
                  >
                    继续确认
                  </button>
                </div>
              </>
            ) : (
              <>
                <div className="review-waiver-warning">
                  <strong>这是二次确认</strong>
                  <p>
                    豁免后当前任务会被接受，并进入推送流程；问题记录、规范快照哈希、操作者和时间都会写入审计记录。该动作不可撤销。
                  </p>
                  <p>理由：{waiverReason.trim()}</p>
                </div>
                <div className="dialog__actions">
                  <button
                    className="button button--ghost"
                    type="button"
                    onClick={() => setWaiverStep('reason')}
                    disabled={busy}
                  >
                    返回修改
                  </button>
                  <button
                    className="button button--danger"
                    type="button"
                    onClick={() => void confirmWaiver()}
                    disabled={busy}
                  >
                    确认豁免并推送
                  </button>
                </div>
              </>
            )}
          </section>
        </div>
      ) : null}
    </section>
  );
}
