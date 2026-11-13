/**
 * 仓库、凭据、能力与能力 API 白名单配置页。
 *
 * 表单允许人工编辑并显式保存，Secret 输入只提交不读回；能力 API 端点保存后由
 * 任务上下文和 MCP 在授权范围内使用。
 */
import { useEffect, useState } from 'react';

import * as integrationApi from '../api/integrationApi';
import type {
  CapabilityApiEndpoint,
  CapabilityApiEndpointInput,
  CapabilityApiMethod,
  CapabilityStatus,
  CapabilityStatusView,
  CommandError,
  CredentialEnvironment,
  CredentialKind,
  CredentialMetadata,
  GitHubDeviceFlow,
  GitHubDevicePoll,
  GitHubRepository,
  GitHubSession,
  IntegrationWorkspace,
  RepositoryAuthMode,
  RepositoryDraft,
  RepositorySyncPolicy,
  RepositoryVerificationCommand,
  RepositoryValidation,
  SaveCredentialInput,
} from '../api/types';
import { ErrorBanner } from './ErrorBanner';
import { RepositoryVerificationEditor } from './RepositoryVerificationEditor';

interface IntegrationViewProps {
  path: string;
}

interface RepositoryFormState {
  key: string;
  name: string;
  localPath: string;
  remoteUrl: string;
  defaultBranch: string;
  protectedBranches: string;
  authMode: RepositoryAuthMode;
  credentialId: string;
  syncPolicy: RepositorySyncPolicy;
}

interface CredentialFormState {
  name: string;
  provider: string;
  kind: CredentialKind;
  environment: CredentialEnvironment;
  secret: string;
}

interface CapabilityFormState {
  status: CapabilityStatus;
  credentialId: string;
  note: string;
}

interface CapabilityApiEndpointFormState {
  capabilityKey: string;
  name: string;
  baseUrl: string;
  pathPrefix: string;
  allowedMethods: CapabilityApiMethod[];
  credentialId: string;
  authHeader: string;
  authScheme: string;
}

const repositoryKeys = ['server', 'admin', 'web', 'app', 'mini_program'];

const capabilityApiMethods: CapabilityApiMethod[] = [
  'GET',
  'POST',
  'PUT',
  'PATCH',
  'DELETE',
];

const repositoryKeyLabels: Record<string, string> = {
  server: 'server / 服务端',
  admin: 'admin / 管理后台',
  web: 'web / Web 前台',
  app: 'app / 移动 App',
  mini_program: 'mini_program / 小程序',
};

const credentialKindLabels: Record<CredentialKind, string> = {
  api_key: 'API Key',
  oauth_token: 'OAuth Token',
  personal_access_token: 'Personal Access Token',
  username_password: '用户名与密码',
  signing_certificate: '签名证书',
  other: '其他',
};

const credentialEnvironmentLabels: Record<CredentialEnvironment, string> = {
  shared: '共享',
  development: '开发',
  staging: '预发布',
  production: '生产',
};

const capabilityStatusLabels: Record<CapabilityStatus, string> = {
  missing: '未配置',
  configured: '已配置',
  verified: '已验证',
  auth_required: '待授权',
  disabled: '已停用',
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

function blankRepositoryForm(): RepositoryFormState {
  return {
    key: 'web',
    name: '',
    localPath: '',
    remoteUrl: '',
    defaultBranch: 'main',
    protectedBranches: 'main',
    authMode: 'none',
    credentialId: '',
    syncPolicy: 'fetch_before_claim',
  };
}

function blankCredentialForm(): CredentialFormState {
  return {
    name: '',
    provider: '',
    kind: 'api_key',
    environment: 'development',
    secret: '',
  };
}

function capabilityKey(item: CapabilityStatusView) {
  return `${item.capabilityId}:${item.environment}`;
}

function blankCapabilityApiEndpointForm(
  capabilities: CapabilityStatusView[] = [],
): CapabilityApiEndpointFormState {
  return {
    capabilityKey: capabilities[0] ? capabilityKey(capabilities[0]) : '',
    name: '',
    baseUrl: '',
    pathPrefix: '/v1',
    allowedMethods: ['GET'],
    credentialId: '',
    authHeader: 'Authorization',
    authScheme: 'Bearer',
  };
}

function capabilityApiEndpointForm(
  endpoint: CapabilityApiEndpoint,
): CapabilityApiEndpointFormState {
  return {
    capabilityKey: `${endpoint.capabilityId}:${endpoint.environment}`,
    name: endpoint.name,
    baseUrl: endpoint.baseUrl,
    pathPrefix: endpoint.pathPrefix,
    allowedMethods: endpoint.allowedMethods,
    credentialId: endpoint.credentialId ?? '',
    authHeader: endpoint.authHeader ?? '',
    authScheme: endpoint.authScheme ?? '',
  };
}

function capabilityDrafts(
  capabilities: CapabilityStatusView[],
): Record<string, CapabilityFormState> {
  return Object.fromEntries(
    capabilities.map((item) => [
      `${item.capabilityId}:${item.environment}`,
      {
        status: item.status,
        credentialId: item.credentialId ?? '',
        note: item.note ?? '',
      },
    ]),
  );
}

function repositoryDraft(form: RepositoryFormState): RepositoryDraft {
  return {
    key: form.key,
    name: form.name.trim(),
    localPath: form.localPath.trim(),
    remoteUrl: form.remoteUrl.trim(),
    defaultBranch: form.defaultBranch.trim(),
    protectedBranches: form.protectedBranches
      .split(/[\n,]/)
      .map((branch) => branch.trim())
      .filter((branch) => branch.length > 0),
    authMode: form.authMode,
    credentialId: form.credentialId || null,
    syncPolicy: form.syncPolicy,
  };
}

function formatTimestamp(value: string) {
  return new Date(value).toLocaleString('zh-CN', { hour12: false });
}

export function IntegrationView({ path }: IntegrationViewProps) {
  const [workspace, setWorkspace] = useState<IntegrationWorkspace | null>(null);
  const [loading, setLoading] = useState(true);
  const [busyLabel, setBusyLabel] = useState<string | null>(null);
  const [error, setError] = useState<CommandError | null>(null);
  const [repositoryForm, setRepositoryForm] = useState(blankRepositoryForm);
  const [validation, setValidation] = useState<RepositoryValidation | null>(null);
  const [credentialForm, setCredentialForm] = useState(blankCredentialForm);
  const [capabilityForms, setCapabilityForms] = useState<
    Record<string, CapabilityFormState>
  >({});
  const [endpointForm, setEndpointForm] = useState<CapabilityApiEndpointFormState>(
    blankCapabilityApiEndpointForm,
  );
  const [editingEndpointId, setEditingEndpointId] = useState<string | null>(null);
  const [githubSession, setGitHubSession] = useState<GitHubSession | null>(null);
  const [clientId, setClientId] = useState('');
  const [deviceFlow, setDeviceFlow] = useState<GitHubDeviceFlow | null>(null);
  const [deviceStatus, setDeviceStatus] = useState<string | null>(null);
  const [privateRepositoryName, setPrivateRepositoryName] = useState('');
  const [privateRepositoryDescription, setPrivateRepositoryDescription] = useState('');
  const [privateRepositoryKey, setPrivateRepositoryKey] = useState('');
  const [privateCredentialId, setPrivateCredentialId] = useState('');
  const [confirmPrivateCreation, setConfirmPrivateCreation] = useState(false);
  const [createdRepository, setCreatedRepository] = useState<GitHubRepository | null>(
    null,
  );

  function adoptWorkspace(next: IntegrationWorkspace) {
    setWorkspace(next);
    setCapabilityForms(capabilityDrafts(next.capabilities));
    setEndpointForm((current) =>
      current.capabilityKey || next.capabilities.length === 0
        ? current
        : blankCapabilityApiEndpointForm(next.capabilities),
    );
    setPrivateRepositoryKey((current) => current || next.repositories[0]?.key || '');
    setPrivateCredentialId((current) => current || next.credentials[0]?.id || '');
  }

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    integrationApi
      .readIntegrationWorkspace(path)
      .then((next) => {
        if (!cancelled) {
          adoptWorkspace(next);
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

  async function perform<T>(label: string, action: () => Promise<T>) {
    if (busyLabel) {
      return null;
    }
    setBusyLabel(label);
    setError(null);
    try {
      return await action();
    } catch (actionError) {
      setError(normalizeCommandError(actionError));
      return null;
    } finally {
      setBusyLabel(null);
    }
  }

  async function refreshWorkspace() {
    const next = await integrationApi.readIntegrationWorkspace(path);
    adoptWorkspace(next);
    return next;
  }

  async function handleValidateRepository() {
    const result = await perform('正在验证仓库', () =>
      integrationApi.validateRepository(path, repositoryDraft(repositoryForm)),
    );
    if (result) {
      setValidation(result);
    }
  }

  async function handleSaveRepository() {
    const result = await perform('正在保存仓库', () =>
      integrationApi.saveRepository(path, repositoryDraft(repositoryForm)),
    );
    if (result) {
      adoptWorkspace(result);
      setValidation(null);
    }
  }

  async function handleSaveVerificationCommands(
    repositoryKey: string,
    commands: RepositoryVerificationCommand[],
  ) {
    const saved = await perform('正在保存仓库验证命令', () =>
      integrationApi.saveRepositoryVerificationCommands(
        path,
        repositoryKey,
        commands,
      ),
    );
    if (!saved) {
      return false;
    }
    await perform('正在刷新集成信息', refreshWorkspace);
    return true;
  }

  async function handleSaveCredential() {
    const input: SaveCredentialInput = {
      name: credentialForm.name.trim(),
      provider: credentialForm.provider.trim(),
      kind: credentialForm.kind,
      environment: credentialForm.environment,
      secret: credentialForm.secret,
    };
    const result = await perform('正在保存凭据', () =>
      integrationApi.saveCredential(path, input),
    );
    if (result) {
      adoptWorkspace(result);
      setCredentialForm(blankCredentialForm());
    }
  }

  async function handleDeleteCredential(item: CredentialMetadata) {
    const result = await perform('正在删除凭据', () =>
      integrationApi.deleteCredential(path, item.id),
    );
    if (result) {
      adoptWorkspace(result);
    }
  }

  async function handleUpdateCapability(item: CapabilityStatusView) {
    const key = `${item.capabilityId}:${item.environment}`;
    const draft = capabilityForms[key] ?? {
      status: item.status,
      credentialId: item.credentialId ?? '',
      note: item.note ?? '',
    };
    const result = await perform('正在更新能力状态', () =>
      integrationApi.updateCapability(
        path,
        item.capabilityId,
        item.environment,
        draft.status,
        draft.credentialId || null,
        draft.note.trim() || null,
      ),
    );
    if (result) {
      adoptWorkspace(result);
    }
  }

  function beginNewEndpoint() {
    setEditingEndpointId(null);
    setEndpointForm(blankCapabilityApiEndpointForm(workspace?.capabilities ?? []));
  }

  function beginEditEndpoint(item: CapabilityApiEndpoint) {
    setEditingEndpointId(item.id);
    setEndpointForm(capabilityApiEndpointForm(item));
  }

  function toggleEndpointMethod(method: CapabilityApiMethod) {
    setEndpointForm((current) => ({
      ...current,
      allowedMethods: current.allowedMethods.includes(method)
        ? current.allowedMethods.filter((value) => value !== method)
        : [...current.allowedMethods, method],
    }));
  }

  async function handleSaveEndpoint() {
    const [capabilityId, environment] = endpointForm.capabilityKey.split(':');
    if (!capabilityId || !environment) {
      return;
    }
    const input: CapabilityApiEndpointInput = {
      capabilityId,
      environment,
      name: endpointForm.name.trim(),
      baseUrl: endpointForm.baseUrl.trim(),
      pathPrefix: endpointForm.pathPrefix.trim(),
      allowedMethods: endpointForm.allowedMethods,
      credentialId: endpointForm.credentialId || null,
      authHeader: endpointForm.authHeader.trim() || null,
      authScheme: endpointForm.authScheme.trim() || null,
    };
    const result = await perform('正在保存能力 API 白名单', () =>
      integrationApi.saveCapabilityApiEndpoint(path, editingEndpointId, input),
    );
    if (result) {
      adoptWorkspace(result);
      setEditingEndpointId(null);
      setEndpointForm(blankCapabilityApiEndpointForm(result.capabilities));
    }
  }

  async function handleDeleteEndpoint(item: CapabilityApiEndpoint) {
    const result = await perform('正在删除能力 API 白名单', () =>
      integrationApi.deleteCapabilityApiEndpoint(path, item.id),
    );
    if (result) {
      adoptWorkspace(result);
      if (editingEndpointId === item.id) {
        setEditingEndpointId(null);
        setEndpointForm(blankCapabilityApiEndpointForm(result.capabilities));
      }
    }
  }

  async function handleImportGitHubSession() {
    const session = await perform('正在导入 GitHub 登录', () =>
      integrationApi.importGitHubSession(path),
    );
    if (session) {
      setGitHubSession(session);
      await perform('正在刷新集成信息', refreshWorkspace);
    }
  }

  async function handleStartGitHubFlow() {
    const flow = await perform('正在启动 GitHub 授权', () =>
      integrationApi.startGitHubDeviceFlow(clientId.trim()),
    );
    if (flow) {
      setDeviceFlow(flow);
      setDeviceStatus(null);
    }
  }

  async function handlePollGitHubFlow() {
    if (!deviceFlow) {
      return;
    }
    const result = await perform('正在检查 GitHub 授权', () =>
      integrationApi.pollGitHubDeviceFlow(path, deviceFlow.sessionId),
    );
    if (!result) {
      return;
    }
    handleDevicePoll(result);
    if (result.state === 'authorized') {
      await perform('正在刷新集成信息', refreshWorkspace);
    }
  }

  function handleDevicePoll(result: GitHubDevicePoll) {
    if (result.state === 'authorized') {
      setGitHubSession(result.session);
      setDeviceFlow(null);
      setDeviceStatus('GitHub 授权成功');
    } else if (result.state === 'pending') {
      setDeviceStatus(`等待授权，建议 ${result.interval} 秒后再次检查`);
    } else if (result.state === 'slow_down') {
      setDeviceStatus(`GitHub 要求降低轮询频率，请 ${result.interval} 秒后再试`);
    } else if (result.state === 'access_denied') {
      setDeviceStatus('授权已被拒绝');
      setDeviceFlow(null);
    } else {
      setDeviceStatus('设备码已过期，请重新启动授权');
      setDeviceFlow(null);
    }
  }

  function requestPrivateRepositoryCreation() {
    if (
      privateRepositoryName.trim() &&
      privateRepositoryKey &&
      privateCredentialId
    ) {
      setCreatedRepository(null);
      setConfirmPrivateCreation(true);
    }
  }

  async function handleConfirmPrivateRepository() {
    setConfirmPrivateCreation(false);
    const created = await perform('正在创建 GitHub 私有仓库', () =>
      integrationApi.createGitHubPrivateRepository(path, {
        credentialId: privateCredentialId,
        repositoryKey: privateRepositoryKey,
        name: privateRepositoryName.trim(),
        description: privateRepositoryDescription.trim() || null,
      }),
    );
    if (created) {
      setCreatedRepository(created);
      setPrivateRepositoryName('');
      setPrivateRepositoryDescription('');
      await perform('正在刷新集成信息', refreshWorkspace);
    }
  }

  const busy = busyLabel !== null;
  const configuredCapabilities = workspace?.capabilities.filter(
    (item) => item.status === 'configured' || item.status === 'verified',
  ).length;
  const failedSyncs = workspace?.gitSyncs.filter((item) => item.status === 'failed').length;

  if (loading) {
    return (
      <section className="content-view integration-view" aria-busy="true">
        <p className="panel__eyebrow">仓库与能力</p>
        <h1>仓库与能力</h1>
        <div className="empty-state">正在读取本地仓库、凭据与能力状态…</div>
      </section>
    );
  }

  return (
    <section className="content-view integration-view" aria-busy={busy}>
      <header className="content-view__header">
        <div>
          <p className="eyebrow">仓库与三方能力</p>
          <h1>仓库与能力</h1>
          <p className="content-view__lead">
            把逻辑仓库映射到本地 Git 工作区，把 Secret 写入 Windows Credential
            Manager，并把三方能力状态交给 Codex 读取。MCP 永远拿不到明文 Secret。
          </p>
        </div>
      </header>

      {error ? <ErrorBanner error={error} onDismiss={() => setError(null)} /> : null}

      {workspace ? (
        <>
          <div className="plan-metrics integration-metrics">
            <article>
              <span>仓库映射</span>
              <strong>{workspace.repositories.length}</strong>
            </article>
            <article>
              <span>系统凭据</span>
              <strong>{workspace.credentials.length}</strong>
            </article>
            <article>
              <span>已配置能力</span>
              <strong>{configuredCapabilities}</strong>
            </article>
            <article className={failedSyncs ? 'metric-card--attention' : ''}>
              <span>推送风险</span>
              <strong>{failedSyncs}</strong>
            </article>
          </div>

          {busyLabel ? (
            <p className="integration-busy" role="status">
              {busyLabel}…
            </p>
          ) : null}

          <div className="integration-layout">
            <section className="panel" aria-labelledby="repositories-title">
              <div className="panel__head">
                <div>
                  <p className="panel__eyebrow">仓库映射</p>
                  <h2 id="repositories-title">代码仓库</h2>
                </div>
                <span className="count-badge">{workspace.repositories.length}</span>
              </div>

              {workspace.repositories.length ? (
                <div className="repository-list">
                  {workspace.repositories.map((item) => (
                    <article className="integration-card" key={item.id}>
                      <header>
                        <div>
                          <strong>{item.name}</strong>
                          <code>{item.key}</code>
                        </div>
                        <span>{item.defaultBranch}</span>
                      </header>
                      <p className="mono integration-path">{item.localPath}</p>
                      <p className="integration-muted">{item.remoteUrl}</p>
                      <div className="integration-tags">
                        <span>保护：{item.protectedBranches.join(', ')}</span>
                        <span>{item.authMode}</span>
                        <span>{item.syncPolicy}</span>
                      </div>
                    </article>
                  ))}
                </div>
              ) : (
                <div className="empty-state">
                  <strong>还没有映射仓库</strong>
                  <p>先保存一个现有 Git 工作区，领取任务时才会创建 codex/* 分支。</p>
                </div>
              )}

              <form
                className="integration-form"
                onSubmit={(event) => {
                  event.preventDefault();
                  void handleSaveRepository();
                }}
              >
                <div className="integration-form__grid">
                  <label className="field">
                    <span>仓库逻辑键</span>
                    <select
                      value={repositoryForm.key}
                      onChange={(event) =>
                        setRepositoryForm({ ...repositoryForm, key: event.target.value })
                      }
                      disabled={busy}
                    >
                      {repositoryKeys.map((key) => (
                        <option value={key} key={key}>
                          {repositoryKeyLabels[key]}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="field">
                    <span>仓库名称</span>
                    <input
                      value={repositoryForm.name}
                      onChange={(event) =>
                        setRepositoryForm({ ...repositoryForm, name: event.target.value })
                      }
                      placeholder="Web 前台"
                      disabled={busy}
                    />
                  </label>
                  <label className="field integration-form__wide">
                    <span>本地路径</span>
                    <input
                      value={repositoryForm.localPath}
                      onChange={(event) =>
                        setRepositoryForm({
                          ...repositoryForm,
                          localPath: event.target.value,
                        })
                      }
                      placeholder={String.raw`D:\work\project-web`}
                      disabled={busy}
                    />
                  </label>
                  <label className="field integration-form__wide">
                    <span>远程 URL</span>
                    <input
                      value={repositoryForm.remoteUrl}
                      onChange={(event) =>
                        setRepositoryForm({
                          ...repositoryForm,
                          remoteUrl: event.target.value,
                        })
                      }
                      placeholder="https://github.com/owner/project.git"
                      disabled={busy}
                    />
                  </label>
                  <label className="field">
                    <span>默认分支</span>
                    <input
                      value={repositoryForm.defaultBranch}
                      onChange={(event) =>
                        setRepositoryForm({
                          ...repositoryForm,
                          defaultBranch: event.target.value,
                        })
                      }
                      disabled={busy}
                    />
                  </label>
                  <label className="field">
                    <span>保护分支（逗号分隔）</span>
                    <input
                      value={repositoryForm.protectedBranches}
                      onChange={(event) =>
                        setRepositoryForm({
                          ...repositoryForm,
                          protectedBranches: event.target.value,
                        })
                      }
                      disabled={busy}
                    />
                  </label>
                  <label className="field">
                    <span>认证方式</span>
                    <select
                      value={repositoryForm.authMode}
                      onChange={(event) =>
                        setRepositoryForm({
                          ...repositoryForm,
                          authMode: event.target.value as RepositoryAuthMode,
                          credentialId:
                            event.target.value === 'credential'
                              ? repositoryForm.credentialId
                              : '',
                        })
                      }
                      disabled={busy}
                    >
                      <option value="none">无</option>
                      <option value="credential">系统凭据</option>
                      <option value="ssh_agent">SSH Agent</option>
                    </select>
                  </label>
                  <label className="field">
                    <span>关联凭据</span>
                    <select
                      value={repositoryForm.credentialId}
                      onChange={(event) =>
                        setRepositoryForm({
                          ...repositoryForm,
                          credentialId: event.target.value,
                        })
                      }
                      disabled={busy || repositoryForm.authMode !== 'credential'}
                    >
                      <option value="">不关联</option>
                      {workspace.credentials.map((item) => (
                        <option value={item.id} key={item.id}>
                          {item.name} · {item.provider}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="field">
                    <span>同步策略</span>
                    <select
                      value={repositoryForm.syncPolicy}
                      onChange={(event) =>
                        setRepositoryForm({
                          ...repositoryForm,
                          syncPolicy: event.target.value as RepositorySyncPolicy,
                        })
                      }
                      disabled={busy}
                    >
                      <option value="fetch_before_claim">领取前 fetch</option>
                      <option value="manual">手动同步</option>
                    </select>
                  </label>
                </div>
                <p className="field__hint">
                  Git 由系统 git 子进程执行，只允许普通推送；保护分支和强推会被拒绝。
                </p>
                <div className="view-actions integration-form__actions">
                  <button
                    className="button button--secondary"
                    type="button"
                    onClick={() => void handleValidateRepository()}
                    disabled={busy}
                  >
                    验证仓库
                  </button>
                  <button
                    className="button button--primary"
                    type="submit"
                    disabled={busy}
                  >
                    保存仓库
                  </button>
                </div>
                {validation ? (
                  <div className="integration-result" role="status">
                    <strong>{validation.repositoryKey}</strong>
                    <span>分支：{validation.status.branch ?? '未命名'}</span>
                    <span>工作区：{validation.status.clean ? '干净' : '有修改'}</span>
                  </div>
                ) : null}
              </form>

              <RepositoryVerificationEditor
                repositories={workspace.repositories}
                configurations={workspace.verificationCommands}
                busy={busy}
                onSave={handleSaveVerificationCommands}
              />

              <div className="integration-subsection">
                <div>
                  <p className="panel__eyebrow">GitHub 私有仓库</p>
                  <h3>创建 GitHub 私有仓库</h3>
                  <p className="field__hint">
                    只在桌面界面明确确认后调用 GitHub REST API，再更新本地 origin。MCP 不提供仓库 CRUD。
                  </p>
                </div>
                <div className="integration-form__grid">
                  <label className="field">
                    <span>目标仓库</span>
                    <select
                      value={
                        privateRepositoryKey || workspace.repositories[0]?.key || ''
                      }
                      onChange={(event) => setPrivateRepositoryKey(event.target.value)}
                      disabled={busy || workspace.repositories.length === 0}
                    >
                      {workspace.repositories.map((item) => (
                        <option value={item.key} key={item.key}>
                          {item.name} · {item.key}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="field">
                    <span>GitHub 凭据</span>
                    <select
                      value={privateCredentialId || workspace.credentials[0]?.id || ''}
                      onChange={(event) => setPrivateCredentialId(event.target.value)}
                      disabled={busy || workspace.credentials.length === 0}
                    >
                      {workspace.credentials.map((item) => (
                        <option value={item.id} key={item.id}>
                          {item.name} · {item.provider}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="field">
                    <span>私有仓库名称</span>
                    <input
                      value={privateRepositoryName}
                      onChange={(event) => setPrivateRepositoryName(event.target.value)}
                      placeholder="coldstart-demo"
                      disabled={busy}
                    />
                  </label>
                  <label className="field integration-form__wide">
                    <span>仓库描述</span>
                    <textarea
                      value={privateRepositoryDescription}
                      onChange={(event) =>
                        setPrivateRepositoryDescription(event.target.value)
                      }
                      disabled={busy}
                      rows={2}
                    />
                  </label>
                </div>
                <button
                  className="button button--secondary"
                  type="button"
                  onClick={requestPrivateRepositoryCreation}
                  disabled={
                    busy ||
                    !privateRepositoryName.trim() ||
                    !privateRepositoryKey ||
                    !privateCredentialId
                  }
                >
                  创建 GitHub 私有仓库
                </button>
                {createdRepository ? (
                  <p className="integration-result integration-result--success" role="status">
                    <strong>{createdRepository.fullName}</strong>
                    <span>已创建私有仓库并更新本地 origin</span>
                  </p>
                ) : null}
              </div>
            </section>
            <section className="panel" aria-labelledby="credentials-title">
              <div className="panel__head">
                <div>
                  <p className="panel__eyebrow">凭据 · Windows 凭据管理器</p>
                  <h2 id="credentials-title">系统凭据</h2>
                </div>
                <span className="count-badge">{workspace.credentials.length}</span>
              </div>

              {workspace.credentials.length ? (
                <div className="credential-list">
                  {workspace.credentials.map((item) => (
                    <article className="integration-card" key={item.id}>
                      <header>
                        <div>
                          <strong>{item.name}</strong>
                          <code>{item.provider}</code>
                        </div>
                        <button
                          className="button button--ghost button--compact"
                          type="button"
                          aria-label={`删除凭据：${item.name}`}
                          onClick={() => void handleDeleteCredential(item)}
                          disabled={busy}
                        >
                          删除
                        </button>
                      </header>
                      <div className="integration-tags">
                        <span>{credentialKindLabels[item.kind]}</span>
                        <span>{credentialEnvironmentLabels[item.environment]}</span>
                        <span>已写入系统凭据库</span>
                      </div>
                      <p className="integration-muted">
                        更新于 {formatTimestamp(item.updatedAt)}
                      </p>
                    </article>
                  ))}
                </div>
              ) : (
                <div className="empty-state">
                  <strong>还没有系统凭据</strong>
                  <p>凭据载荷只写入 Windows Credential Manager，项目数据库仅保留引用。</p>
                </div>
              )}

              <form
                className="integration-form"
                onSubmit={(event) => {
                  event.preventDefault();
                  void handleSaveCredential();
                }}
              >
                <div className="integration-form__grid">
                  <label className="field">
                    <span>凭据名称</span>
                    <input
                      value={credentialForm.name}
                      onChange={(event) =>
                        setCredentialForm({
                          ...credentialForm,
                          name: event.target.value,
                        })
                      }
                      placeholder="GitHub 发布账号"
                      autoComplete="off"
                      disabled={busy}
                    />
                  </label>
                  <label className="field">
                    <span>提供方</span>
                    <input
                      value={credentialForm.provider}
                      onChange={(event) =>
                        setCredentialForm({
                          ...credentialForm,
                          provider: event.target.value,
                        })
                      }
                      placeholder="github / aliyun"
                      autoComplete="off"
                      disabled={busy}
                    />
                  </label>
                  <label className="field">
                    <span>凭据类型</span>
                    <select
                      value={credentialForm.kind}
                      onChange={(event) =>
                        setCredentialForm({
                          ...credentialForm,
                          kind: event.target.value as CredentialKind,
                        })
                      }
                      disabled={busy}
                    >
                      {Object.entries(credentialKindLabels).map(([value, label]) => (
                        <option value={value} key={value}>
                          {label}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="field">
                    <span>使用环境</span>
                    <select
                      value={credentialForm.environment}
                      onChange={(event) =>
                        setCredentialForm({
                          ...credentialForm,
                          environment: event.target.value as CredentialEnvironment,
                        })
                      }
                      disabled={busy}
                    >
                      {Object.entries(credentialEnvironmentLabels).map(
                        ([value, label]) => (
                          <option value={value} key={value}>
                            {label}
                          </option>
                        ),
                      )}
                    </select>
                  </label>
                  <label className="field integration-form__wide">
                    <span>Secret（仅写入 Windows 凭据库）</span>
                    <input
                      type="password"
                      value={credentialForm.secret}
                      onChange={(event) =>
                        setCredentialForm({
                          ...credentialForm,
                          secret: event.target.value,
                        })
                      }
                      placeholder="粘贴后保存；界面不会回读明文"
                      autoComplete="new-password"
                      spellCheck={false}
                      disabled={busy}
                    />
                  </label>
                </div>
                <p className="field__hint">
                  保存后输入框立即清空。Secret 不进入 SQLite、事件、日志、MCP 上下文或错误消息。
                </p>
                <div className="view-actions integration-form__actions">
                  <button
                    className="button button--primary"
                    type="submit"
                    disabled={
                      busy ||
                      !credentialForm.name.trim() ||
                      !credentialForm.provider.trim() ||
                      !credentialForm.secret
                    }
                  >
                    保存凭据
                  </button>
                </div>
              </form>

              <div className="integration-subsection">
                <div>
                  <p className="panel__eyebrow">GitHub 授权</p>
                  <h3>GitHub 账号</h3>
                  <p className="field__hint">
                    优先导入本机 gh 登录；也可用 OAuth Device Flow 在浏览器授权。Token 只保存到系统凭据库。
                  </p>
                </div>
                <div className="view-actions integration-form__actions">
                  <button
                    className="button button--secondary"
                    type="button"
                    onClick={() => void handleImportGitHubSession()}
                    disabled={busy}
                  >
                    从 gh 导入 GitHub 登录
                  </button>
                </div>
                {githubSession ? (
                  <p className="integration-result integration-result--success" role="status">
                    <strong>@{githubSession.login}</strong>
                    <span>已连接 GitHub，授权已写入系统凭据库</span>
                  </p>
                ) : null}
                <div className="integration-form__grid">
                  <label className="field integration-form__wide">
                    <span>GitHub OAuth Client ID</span>
                    <input
                      value={clientId}
                      onChange={(event) => setClientId(event.target.value)}
                      placeholder="GitHub OAuth App 的 Client ID"
                      autoComplete="off"
                      spellCheck={false}
                      disabled={busy}
                    />
                  </label>
                </div>
                <button
                  className="button button--secondary"
                  type="button"
                  onClick={() => void handleStartGitHubFlow()}
                  disabled={busy || !clientId.trim()}
                >
                  启动 GitHub 授权
                </button>
                {deviceFlow ? (
                  <div className="integration-device-flow" role="status">
                    <p>在 GitHub 输入以下设备码：</p>
                    <strong className="mono">{deviceFlow.userCode}</strong>
                    <a href={deviceFlow.verificationUri} target="_blank" rel="noreferrer">
                      {deviceFlow.verificationUri}
                    </a>
                    <button
                      className="button button--primary"
                      type="button"
                      onClick={() => void handlePollGitHubFlow()}
                      disabled={busy}
                    >
                      检查 GitHub 授权状态
                    </button>
                  </div>
                ) : null}
                {deviceStatus ? (
                  <p className="integration-muted" role="status">
                    {deviceStatus}
                  </p>
                ) : null}
              </div>
            </section>
          </div>
          <section
            className="panel integration-panel--wide"
            aria-labelledby="capabilities-title"
          >
            <div className="panel__head">
              <div>
                <p className="panel__eyebrow">三方能力矩阵</p>
                <h2 id="capabilities-title">三方能力状态</h2>
              </div>
              <span className="count-badge">{workspace.capabilities.length}</span>
            </div>

            {workspace.capabilities.length ? (
              <div className="capability-grid">
                {workspace.capabilities.map((item) => {
                  const key = `${item.capabilityId}:${item.environment}`;
                  const draft = capabilityForms[key] ?? {
                    status: item.status,
                    credentialId: item.credentialId ?? '',
                    note: item.note ?? '',
                  };
                  return (
                    <article className="capability-card" key={key}>
                      <header>
                        <div>
                          <strong>{item.name}</strong>
                          <code>{item.provider}</code>
                        </div>
                        <span
                          className={`integration-status integration-status--${item.status}`}
                        >
                          {capabilityStatusLabels[item.status]}
                        </span>
                      </header>
                      <p className="integration-muted">
                        {item.environment} · 状态值 <code>{item.status}</code>
                      </p>
                      <div className="integration-form__grid">
                        <label className="field">
                          <span>{`能力状态：${item.name} / ${item.environment}`}</span>
                          <select
                            value={draft.status}
                            onChange={(event) =>
                              setCapabilityForms({
                                ...capabilityForms,
                                [key]: {
                                  ...draft,
                                  status: event.target.value as CapabilityStatus,
                                },
                              })
                            }
                            disabled={busy}
                          >
                            {Object.entries(capabilityStatusLabels).map(
                              ([value, label]) => (
                                <option value={value} key={value}>
                                  {label}
                                </option>
                              ),
                            )}
                          </select>
                        </label>
                        <label className="field">
                          <span>{`关联凭据：${item.name}`}</span>
                          <select
                            value={draft.credentialId}
                            onChange={(event) =>
                              setCapabilityForms({
                                ...capabilityForms,
                                [key]: {
                                  ...draft,
                                  credentialId: event.target.value,
                                },
                              })
                            }
                            disabled={busy}
                          >
                            <option value="">不关联</option>
                            {workspace.credentials.map((credential) => (
                              <option value={credential.id} key={credential.id}>
                                {credential.name} · {credential.provider}
                              </option>
                            ))}
                          </select>
                        </label>
                        <label className="field integration-form__wide">
                          <span>{`备注：${item.name}`}</span>
                          <textarea
                            value={draft.note}
                            onChange={(event) =>
                              setCapabilityForms({
                                ...capabilityForms,
                                [key]: { ...draft, note: event.target.value },
                              })
                            }
                            rows={2}
                            disabled={busy}
                          />
                        </label>
                      </div>
                      <button
                        className="button button--secondary"
                        type="button"
                        aria-label={`更新能力：${item.name} / ${item.environment}`}
                        onClick={() => void handleUpdateCapability(item)}
                        disabled={busy}
                      >
                        更新能力状态
                      </button>
                    </article>
                  );
                })}
              </div>
            ) : (
              <div className="empty-state">
                <strong>当前计划没有三方能力要求</strong>
                <p>蓝图声明文件存储、短信、支付等能力后，会在这里按环境跟踪配置状态。</p>
              </div>
            )}
          </section>

          <section
            className="panel integration-panel--wide"
            aria-labelledby="capability-api-title"
          >
            <div className="panel__head">
              <div>
                <p className="panel__eyebrow">MCP 受限凭据代理</p>
                <h2 id="capability-api-title">能力 API 白名单</h2>
              </div>
              <span className="count-badge">
                {workspace.capabilityApiEndpoints.length}
              </span>
            </div>
            <p className="field__hint">
              Codex 只能通过 MCP 调用任务已授权的端点。Secret 由 Windows
              凭据库在本机注入，页面、MCP 参数与响应都不回显明文；第一版只支持静态鉴权
              Header。
            </p>

            {workspace.capabilityApiEndpoints.length ? (
              <div className="credential-list capability-api-list">
                {workspace.capabilityApiEndpoints.map((item) => {
                  const capabilityName =
                    workspace.capabilities.find(
                      (capability) =>
                        capability.capabilityId === item.capabilityId &&
                        capability.environment === item.environment,
                    )?.name ?? item.capabilityId;
                  const credentialName =
                    workspace.credentials.find(
                      (credential) => credential.id === item.credentialId,
                    )?.name ?? '不关联凭据';
                  return (
                    <article className="integration-card" key={item.id}>
                      <header>
                        <div>
                          <strong>{item.name}</strong>
                          <code>
                            {capabilityName} · {item.environment}
                          </code>
                        </div>
                        <div className="view-actions">
                          <button
                            className="button button--ghost button--compact"
                            type="button"
                            aria-label={`编辑能力 API：${item.name}`}
                            onClick={() => beginEditEndpoint(item)}
                            disabled={busy}
                          >
                            编辑
                          </button>
                          <button
                            className="button button--ghost button--compact"
                            type="button"
                            aria-label={`删除能力 API：${item.name}`}
                            onClick={() => void handleDeleteEndpoint(item)}
                            disabled={busy}
                          >
                            删除
                          </button>
                        </div>
                      </header>
                      <p className="integration-path">
                        {item.baseUrl}
                        {item.pathPrefix}
                      </p>
                      <div className="integration-tags">
                        {item.allowedMethods.map((method) => (
                          <span key={method}>{method}</span>
                        ))}
                        <span>{credentialName}</span>
                        <span>
                          {item.authHeader
                            ? `${item.authHeader}${item.authScheme ? ` · ${item.authScheme}` : ''}`
                            : '无鉴权头'}
                        </span>
                      </div>
                    </article>
                  );
                })}
              </div>
            ) : (
              <div className="empty-state">
                <strong>还没有能力 API 白名单</strong>
                <p>
                  为已配置能力添加 HTTPS 端点后，任务上下文只会展示端点元数据，Codex
                  仍需通过租约和能力授权才能调用。
                </p>
              </div>
            )}

            <div className="integration-subsection">
              <div className="capability-api-editor__head">
                <div>
                  <p className="panel__eyebrow">
                    {editingEndpointId ? '编辑现有端点' : '新增端点'}
                  </p>
                  <h3>{editingEndpointId ? '修改能力 API 白名单' : '登记能力 API'}</h3>
                </div>
                {editingEndpointId ? (
                  <button
                    className="button button--ghost button--compact"
                    type="button"
                    onClick={beginNewEndpoint}
                    disabled={busy}
                  >
                    新建白名单
                  </button>
                ) : null}
              </div>
              {workspace.capabilities.length ? (
                <form
                  className="integration-form capability-api-form"
                  onSubmit={(event) => {
                    event.preventDefault();
                    void handleSaveEndpoint();
                  }}
                >
                  <div className="integration-form__grid">
                    <label className="field integration-form__wide">
                      <span>能力与环境</span>
                      <select
                        value={endpointForm.capabilityKey}
                        onChange={(event) =>
                          setEndpointForm({
                            ...endpointForm,
                            capabilityKey: event.target.value,
                          })
                        }
                        disabled={busy || editingEndpointId !== null}
                      >
                        {workspace.capabilities.map((item) => (
                          <option value={capabilityKey(item)} key={capabilityKey(item)}>
                            {item.name} · {item.environment}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label className="field">
                      <span>白名单名称</span>
                      <input
                        value={endpointForm.name}
                        onChange={(event) =>
                          setEndpointForm({ ...endpointForm, name: event.target.value })
                        }
                        placeholder="附件上传 API"
                        disabled={busy}
                      />
                    </label>
                    <label className="field">
                      <span>路径前缀</span>
                      <input
                        value={endpointForm.pathPrefix}
                        onChange={(event) =>
                          setEndpointForm({
                            ...endpointForm,
                            pathPrefix: event.target.value,
                          })
                        }
                        placeholder="/v1"
                        disabled={busy}
                      />
                    </label>
                    <label className="field integration-form__wide">
                      <span>HTTPS Base URL</span>
                      <input
                        value={endpointForm.baseUrl}
                        onChange={(event) =>
                          setEndpointForm({
                            ...endpointForm,
                            baseUrl: event.target.value,
                          })
                        }
                        placeholder="https://api.example.com"
                        autoComplete="off"
                        spellCheck={false}
                        disabled={busy}
                      />
                    </label>
                    <div className="field integration-form__wide">
                      <span>允许方法</span>
                      <div className="capability-api-methods">
                        {capabilityApiMethods.map((method) => (
                          <label key={method}>
                            <input
                              type="checkbox"
                              aria-label={`允许方法 ${method}`}
                              checked={endpointForm.allowedMethods.includes(method)}
                              onChange={() => toggleEndpointMethod(method)}
                              disabled={busy}
                            />
                            <span>{method}</span>
                          </label>
                        ))}
                      </div>
                    </div>
                    <label className="field">
                      <span>能力 API 凭据</span>
                      <select
                        value={endpointForm.credentialId}
                        onChange={(event) =>
                          setEndpointForm({
                            ...endpointForm,
                            credentialId: event.target.value,
                          })
                        }
                        disabled={busy}
                      >
                        <option value="">不关联凭据</option>
                        {workspace.credentials.map((item) => (
                          <option value={item.id} key={item.id}>
                            {item.name} · {item.environment}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label className="field">
                      <span>鉴权头名称</span>
                      <input
                        value={endpointForm.authHeader}
                        onChange={(event) =>
                          setEndpointForm({
                            ...endpointForm,
                            authHeader: event.target.value,
                          })
                        }
                        placeholder="Authorization"
                        autoComplete="off"
                        spellCheck={false}
                        disabled={busy}
                      />
                    </label>
                    <label className="field">
                      <span>鉴权前缀</span>
                      <input
                        value={endpointForm.authScheme}
                        onChange={(event) =>
                          setEndpointForm({
                            ...endpointForm,
                            authScheme: event.target.value,
                          })
                        }
                        placeholder="Bearer"
                        autoComplete="off"
                        spellCheck={false}
                        disabled={busy}
                      />
                    </label>
                  </div>
                  <p className="field__hint">
                    只允许 HTTPS、路径前缀内的方法和静态 Header 注入；不代签 OSS
                    等 SDK 签名，也不跟随重定向。
                  </p>
                  <div className="view-actions integration-form__actions">
                    <button
                      className="button button--primary"
                      type="submit"
                      disabled={
                        busy ||
                        !endpointForm.capabilityKey ||
                        !endpointForm.name.trim() ||
                        !endpointForm.baseUrl.trim() ||
                        !endpointForm.pathPrefix.trim() ||
                        endpointForm.allowedMethods.length === 0
                      }
                    >
                      {editingEndpointId ? '保存能力 API 修改' : '新增能力 API 白名单'}
                    </button>
                  </div>
                </form>
              ) : (
                <div className="empty-state">
                  先在蓝图中声明三方能力并完成状态同步，之后才能登记能力 API。
                </div>
              )}
            </div>
          </section>

          <section
            className="panel integration-panel--wide"
            aria-labelledby="git-syncs-title"
          >
            <div className="panel__head">
              <div>
                <p className="panel__eyebrow">推送风险 · 只读观察</p>
                <h2 id="git-syncs-title">Git 同步记录</h2>
              </div>
              <span className="count-badge">{workspace.gitSyncs.length}</span>
            </div>
            {workspace.gitSyncs.length ? (
              <div className="sync-list">
                {workspace.gitSyncs.map((item) => {
                  const repositoryName =
                    workspace.repositories.find(
                      (repository) => repository.id === item.repositoryId,
                    )?.name ?? item.repositoryId;
                  return (
                    <article className="integration-card" key={item.id}>
                      <header>
                        <div>
                          <strong>{repositoryName}</strong>
                          <code>{item.branch}</code>
                        </div>
                        <span
                          className={`integration-status integration-status--${item.status}`}
                        >
                          {item.status === 'pushed' ? '已推送' : '推送失败'}
                        </span>
                      </header>
                      <p className="mono integration-muted">
                        {item.commitSha.slice(0, 12)}
                      </p>
                      {item.errorMessage ? (
                        <p className="integration-error">{item.errorMessage}</p>
                      ) : null}
                      <p className="integration-muted">
                        记录于 {formatTimestamp(item.createdAt)}
                      </p>
                    </article>
                  );
                })}
              </div>
            ) : (
              <div className="empty-state">
                <strong>还没有 Git 推送记录</strong>
                <p>任务验收后会记录普通推送结果；失败不会回滚已完成任务，但会在此保留风险。</p>
              </div>
            )}
          </section>
        </>
      ) : null}

      {confirmPrivateCreation ? (
        <div className="dialog-backdrop" role="presentation">
          <section
            className="dialog integration-confirm"
            role="dialog"
            aria-modal="true"
            aria-labelledby="private-repository-dialog-title"
          >
            <div className="dialog__head">
              <div>
                <p className="eyebrow">外部写入 · 需要明确确认</p>
                <h2 id="private-repository-dialog-title">
                  确认创建 GitHub 私有仓库
                </h2>
              </div>
              <span className="dialog__step">!</span>
            </div>
            <p className="content-view__lead">
              将为本地仓库 <code>{privateRepositoryKey}</code> 调用 GitHub API 创建私有仓库
              <strong>{privateRepositoryName}</strong>，随后把 origin 更新为新仓库地址。
            </p>
            <div className="dialog__actions">
              <button
                className="button button--ghost"
                type="button"
                onClick={() => setConfirmPrivateCreation(false)}
              >
                取消
              </button>
              <button
                className="button button--primary"
                type="button"
                onClick={() => void handleConfirmPrivateRepository()}
              >
                确认创建
              </button>
            </div>
          </section>
        </div>
      ) : null}
    </section>
  );
}
