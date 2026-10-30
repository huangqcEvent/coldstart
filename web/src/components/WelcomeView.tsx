interface WelcomeViewProps {
  onCreate: () => void;
  onOpen: () => void;
}

export function WelcomeView({ onCreate, onOpen }: WelcomeViewProps) {
  return (
    <section className="welcome" aria-labelledby="welcome-title">
      <div className="welcome__copy">
        <p className="eyebrow">COLD BLUE WORKSPACE / 01</p>
        <h1 id="welcome-title">冷启动</h1>
        <p className="welcome__lead">
          把模糊的需求，变成可执行、可追踪、可复盘的工程起点。
        </p>
        <p className="welcome__support">
          项目清单、事件记录与状态判断都留在本地。选择一处主目录，
          从这里开始建立清晰的项目上下文。
        </p>

        <div className="welcome__actions">
          <button className="button button--primary" type="button" onClick={onCreate}>
            新建项目
          </button>
          <button className="button button--secondary" type="button" onClick={onOpen}>
            打开项目
          </button>
        </div>
      </div>

      <div className="welcome__panel" aria-label="当前阶段">
        <div className="welcome__panel-head">
          <span>PHASE 03</span>
          <span className="status-dot">READY</span>
        </div>
        <div className="welcome__orbit" aria-hidden="true">
          <span className="welcome__orbit-core" />
          <span className="welcome__orbit-ring welcome__orbit-ring--one" />
          <span className="welcome__orbit-ring welcome__orbit-ring--two" />
        </div>
        <div className="welcome__panel-copy">
          <p>需求与蓝图</p>
          <span>导入需求，生成并发布可追溯蓝图</span>
        </div>
      </div>
    </section>
  );
}
