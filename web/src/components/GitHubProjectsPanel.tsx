import { useEffect, useId, useState } from 'react';

import * as githubProjectApi from '../api/githubProjectApi';
import type {
  CommandError,
  GitHubProjectItemKind,
  GitHubProjectItemState,
  GitHubProjectOverviewView,
  Repository,
} from '../api/types';

interface GitHubProjectsPanelProps {
  path: string;
  repositories: Repository[];
  busy: boolean;
}

const itemKindLabels: Record<GitHubProjectItemKind, string> = {
  issue: 'Issue',
  pull_request: 'PR',
  draft_issue: '草稿',
  unavailable: '受限',
};

const itemStateLabels: Record<GitHubProjectItemState, string> = {
  open: '开放',
  closed: '已关闭',
  merged: '已合并',
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

function formatTime(value: string | null) {
  if (!value) {
    return null;
  }
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }
  return new Intl.DateTimeFormat('zh-CN', {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(parsed);
}

export function GitHubProjectsPanel({
  path,
  repositories,
  busy,
}: GitHubProjectsPanelProps) {
  const repositoryId = useId();
  const [repositoryKey, setRepositoryKey] = useState('');
  const [overview, setOverview] = useState<GitHubProjectOverviewView | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<CommandError | null>(null);

  useEffect(() => {
    setRepositoryKey((current) =>
      repositories.some((repository) => repository.key === current)
        ? current
        : repositories[0]?.key ?? '',
    );
  }, [repositories]);

  async function handleLoadProjects() {
    if (!repositoryKey || busy || loading) {
      return;
    }
    setLoading(true);
    setError(null);
    try {
      setOverview(await githubProjectApi.listGitHubProjects(path, repositoryKey));
    } catch (loadError) {
      setOverview(null);
      setError(normalizeCommandError(loadError));
    } finally {
      setLoading(false);
    }
  }

  return (
    <section className="panel github-projects" aria-labelledby="github-projects-title">
      <div className="panel__head">
        <div>
          <p className="panel__eyebrow">GitHub 协作视图</p>
          <h2 id="github-projects-title">GitHub Project 只读概览</h2>
          <p className="github-projects__lead">
            读取仓库关联的 Projects v2 与有限条目，帮助确认外部进度。ColdStart
            不会创建或修改项目、字段和条目，也不会把条目导入需求或蓝图。
          </p>
        </div>
        <span className="status-dot">只读</span>
      </div>

      {repositories.length === 0 ? (
        <div className="empty-state">
          <strong>先映射一个 GitHub 仓库</strong>
          <p>到“仓库与能力”配置 GitHub 远程地址和凭据后再读取 Project。</p>
        </div>
      ) : (
        <>
          <div className="github-projects__controls">
            <label className="field" htmlFor={repositoryId}>
              <span>Project 仓库</span>
              <select
                id={repositoryId}
                value={repositoryKey}
                onChange={(event) => {
                  setRepositoryKey(event.target.value);
                  setOverview(null);
                  setError(null);
                }}
                disabled={busy || loading}
              >
                {repositories.map((repository) => (
                  <option key={repository.id} value={repository.key}>
                    {repository.name} · {repository.key}
                  </option>
                ))}
              </select>
            </label>
            <button
              className="button button--secondary"
              type="button"
              onClick={() => void handleLoadProjects()}
              disabled={!repositoryKey || busy || loading}
            >
              {loading ? '正在读取…' : '读取 Project'}
            </button>
          </div>

          <p className="field__hint">
            每次最多读取 5 个项目，每个项目最多 30 个条目和 10 个单选项字段。
            OAuth 授权需要 <code>read:project</code> 权限；PAT 需具备 Projects 只读权限。
          </p>

          {error ? (
            <div className="github-projects__error" role="alert">
              <strong>Project 读取失败</strong>
              <span>{error.message}</span>
              <code>{error.code}</code>
            </div>
          ) : null}

          {overview ? (
            <>
              <div className="github-projects__summary">
                <span>
                  {overview.repositoryLabel} · {overview.projects.length} 个 Project
                </span>
                {overview.truncated ? <strong>项目列表已截断</strong> : null}
              </div>

              {overview.projects.length > 0 ? (
                <div className="github-project-grid">
                  {overview.projects.map((project) => (
                    <article className="github-project" key={project.id}>
                      <header className="github-project__header">
                        <div>
                          <p className="github-project__eyebrow">Project #{project.number}</p>
                          <h3>
                            <a href={project.htmlUrl} target="_blank" rel="noreferrer">
                              {project.title}
                            </a>
                          </h3>
                        </div>
                        <span
                          className={`github-project__state${
                            project.closed ? ' github-project__state--closed' : ''
                          }`}
                        >
                          {project.closed ? '已关闭' : '开放'}
                        </span>
                      </header>
                      <p className="github-project__meta">
                        更新于 {formatTime(project.updatedAt) ?? '未知时间'} ·{' '}
                        {project.totalItems} 个条目
                        {project.truncated ? ' · 当前仅显示前 30 个' : ''}
                      </p>

                      {project.items.length > 0 ? (
                        <ul className="github-project__items" aria-label={`${project.title} 条目`}>
                          {project.items.map((item) => (
                            <li className="github-project-item" key={item.id}>
                              <div className="github-project-item__head">
                                <span
                                  className={`github-project-item__kind github-project-item__kind--${item.kind}`}
                                >
                                  {itemKindLabels[item.kind]}
                                </span>
                                <div className="github-project-item__copy">
                                  <div className="github-project-item__title">
                                    {item.number ? <span>#{item.number}</span> : null}
                                    {item.htmlUrl ? (
                                      <a href={item.htmlUrl} target="_blank" rel="noreferrer">
                                        {item.title}
                                      </a>
                                    ) : (
                                      <strong>{item.title}</strong>
                                    )}
                                  </div>
                                  <p>
                                    {item.state ? itemStateLabels[item.state] : '无内容状态'}
                                    {item.updatedAt
                                      ? ` · 更新于 ${formatTime(item.updatedAt)}`
                                      : ''}
                                  </p>
                                </div>
                              </div>
                              {item.fields.length > 0 ? (
                                <div className="github-project-item__fields" aria-label="项目字段">
                                  {item.fields.map((field, index) => (
                                    <span key={`${field.name}-${index}`}>
                                      <small>{field.name}</small>
                                      {field.value}
                                    </span>
                                  ))}
                                </div>
                              ) : null}
                            </li>
                          ))}
                        </ul>
                      ) : (
                        <div className="empty-state empty-state--compact">
                          <strong>项目还没有可读条目</strong>
                          <p>可能是空项目，或当前凭据只能读取项目元数据。</p>
                        </div>
                      )}
                    </article>
                  ))}
                </div>
              ) : (
                <div className="empty-state">
                  <strong>该仓库没有可见的 Projects v2</strong>
                  <p>确认 Project 已关联此仓库，并检查 Token 是否具备 Projects 只读权限。</p>
                </div>
              )}
            </>
          ) : null}
        </>
      )}
    </section>
  );
}
