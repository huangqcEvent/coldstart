const futureNavigation = [
  '需求导入',
  '蓝图编排',
  '计划引擎',
  'MCP 接入',
  '发布中心',
];

export function Sidebar() {
  return (
    <aside className="sidebar" aria-label="应用导航">
      <div className="sidebar__brand">
        <span className="sidebar__signal" aria-hidden="true" />
        <p>COLDSTART / 冷启动</p>
      </div>

      <div className="sidebar__section-label">WORKSPACE / 阶段导航</div>
      <nav className="sidebar__nav" aria-label="主导航">
        <button
          className="nav-item nav-item--active"
          type="button"
          aria-label="项目总览"
          aria-current="page"
        >
          <span className="nav-item__index">01</span>
          <span>项目总览</span>
        </button>

        {futureNavigation.map((label, index) => (
          <button
            className="nav-item"
            type="button"
            aria-label={label}
            disabled
            key={label}
          >
            <span className="nav-item__index">
              {String(index + 2).padStart(2, '0')}
            </span>
            <span>{label}</span>
            <span className="nav-item__future">后续</span>
          </button>
        ))}
      </nav>

      <div className="sidebar__footer">
        <span className="sidebar__pulse" aria-hidden="true" />
        <div>
          <strong>本地优先</strong>
          <span>数据留在你的工作区</span>
        </div>
      </div>
    </aside>
  );
}
