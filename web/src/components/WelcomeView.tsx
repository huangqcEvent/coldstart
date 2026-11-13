interface WelcomeViewProps {
  onCreate: () => void;
  onOpen: () => void;
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

export function WelcomeView({ onCreate, onOpen }: WelcomeViewProps) {
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
