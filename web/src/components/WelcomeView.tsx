/**
 * 无项目时的欢迎页。
 *
 * 提供打开已有项目与创建新项目的入口，并说明本地优先、MCP 主动领取和人工把关边界。
 */
import type { RecentProjectView } from '../api/types';

interface WelcomeViewProps {
  recentProjects: RecentProjectView[];
  lastProjectPath: string | null;
  restoreNotice: string | null;
  onCreate: () => void;
  onOpen: () => void;
  onOpenRecent: (path: string) => void;
  onForgetRecent: (path: string) => void;
  onClearHistory: () => void;
}

const workflowSteps = [
  {
    title: '整理需求',
    description: '导入现有文档，生成带来源锚点的结构化蓝图。',
  },
  {
    title: '编译计划',
    description: '把模块、依赖、验收标准和人工门禁组织成任务 DAG。',
  },
  {
    title: 'Codex 执行',
    description: '通过 MCP 主动领取任务，只读取当前工作所需的上下文。',
  },
  {
    title: '验证与复盘',
    description: '独立执行验证命令、检查真实 diff，并留下完整项目记忆。',
  },
];

function formatOpenedAt(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return '打开时间未知';
  }
  return date.toLocaleString('zh-CN', {
    month: 'numeric',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function WelcomeView({
  recentProjects,
  lastProjectPath,
  restoreNotice,
  onCreate,
  onOpen,
  onOpenRecent,
  onForgetRecent,
  onClearHistory,
}: WelcomeViewProps) {
  return (
    <section className="welcome" aria-labelledby="welcome-title">
      <div className="welcome__copy">
        <p className="eyebrow">本地优先 · 单机工作区</p>
        <h1 id="welcome-title">冷启动</h1>
        <p className="welcome__lead">
          在写代码之前，先把项目变成清晰、可执行、可追踪的工程起点。
        </p>
        <p className="welcome__support">
          从需求、蓝图和计划开始，把模糊想法整理成 Codex 可以直接执行的上下文。
          密钥、仓库配置与项目记忆都留在本机。
        </p>

        <div className="welcome__actions">
          <button className="button button--primary" type="button" onClick={onCreate}>
            新建项目
          </button>
          <button className="button button--secondary" type="button" onClick={onOpen}>
            打开项目
          </button>
        </div>

        {restoreNotice ? (
          <div className="welcome__history-notice" role="status">
            <strong>{restoreNotice}</strong>
            <span>项目文件没有被删除。可以从下方最近项目重新打开，或移除失效记录。</span>
          </div>
        ) : null}

        {recentProjects.length > 0 ? (
          <section className="welcome__recent" aria-labelledby="recent-projects-title">
            <div className="welcome__recent-head">
              <div>
                <span className="eyebrow" id="recent-projects-title">最近项目</span>
                <strong>继续上次的工程上下文</strong>
              </div>
              <button
                className="button button--quiet"
                type="button"
                onClick={onClearHistory}
              >
                清除访问历史
              </button>
            </div>
            <ul>
              {recentProjects.slice(0, 4).map((project) => {
                const last = project.rootPath === lastProjectPath;
                return (
                  <li
                    className={`welcome__recent-item${project.available ? '' : ' welcome__recent-item--unavailable'}`}
                    key={project.rootPath}
                  >
                    <button
                      className="welcome__recent-main"
                      type="button"
                      aria-label={
                        project.available
                          ? `打开 ${project.name}`
                          : `${project.name}（不可用）`
                      }
                      disabled={!project.available}
                      onClick={() => onOpenRecent(project.rootPath)}
                    >
                      <span className="welcome__recent-copy">
                        <strong>
                          {project.name}
                          {last ? <em>上次打开</em> : null}
                        </strong>
                        <small title={project.rootPath}>{project.rootPath}</small>
                      </span>
                      <span className="welcome__recent-meta">
                        {project.available
                          ? formatOpenedAt(project.lastOpenedAt)
                          : '路径不可用'}
                      </span>
                    </button>
                    <button
                      className="welcome__recent-remove"
                      type="button"
                      aria-label={`从历史记录移除 ${project.name}`}
                      title="仅移除访问记录，不删除项目文件"
                      onClick={() => onForgetRecent(project.rootPath)}
                    >
                      移除
                    </button>
                  </li>
                );
              })}
            </ul>
            {recentProjects.length > 4 ? (
              <p className="welcome__recent-more">
                还有 {recentProjects.length - 4} 个项目，可在左侧项目切换器中查看。
              </p>
            ) : null}
          </section>
        ) : null}

        <ul className="welcome__facts" aria-label="产品特性">
          <li>Windows 桌面</li>
          <li>本地 SQLite</li>
          <li>Codex MCP</li>
        </ul>
      </div>

      <div className="welcome__panel" aria-label="项目工作流">
        <div className="welcome__panel-head">
          <div>
            <span>工作流预览</span>
            <strong>从想法到可执行计划</strong>
          </div>
          <span className="status-dot">本地就绪</span>
        </div>

        <ol className="welcome__flow">
          {workflowSteps.map((step, index) => (
            <li className="welcome__flow-step" key={step.title}>
              <span className="welcome__flow-index">{index + 1}</span>
              <div>
                <strong>{step.title}</strong>
                <p>{step.description}</p>
              </div>
            </li>
          ))}
        </ol>

        <p className="welcome__panel-foot">
          人工只在需要补充信息、做出决策或授权不可逆操作时介入。
        </p>
      </div>
    </section>
  );
}
