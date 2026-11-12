export type WorkspaceView =
  | 'overview'
  | 'requirements'
  | 'blueprint'
  | 'plan'
  | 'inbox'
  | 'integration'
  | 'review'
  | 'release'
  | 'memory';

interface AppSidebarProps {
  view: WorkspaceView;
  hasProject: boolean;
  onNavigate: (view: WorkspaceView) => void;
}

const workspaceItems: Array<{
  view: WorkspaceView;
  label: string;
  index: string;
}> = [
  { view: 'overview', label: '项目总览', index: '01' },
  { view: 'requirements', label: '需求导入', index: '02' },
  { view: 'blueprint', label: '蓝图编排', index: '03' },
  { view: 'inbox', label: '人工 Inbox', index: '04' },
  { view: 'plan', label: '计划看板', index: '05' },
  { view: 'integration', label: '仓库与能力', index: '06' },
  { view: 'review', label: 'AI 评审', index: '07' },
  { view: 'release', label: '发布中心', index: '08' },
  { view: 'memory', label: '项目记忆', index: '09' },
];

export function AppSidebar({ view, hasProject, onNavigate }: AppSidebarProps) {
  return (
    <aside className="sidebar" aria-label="应用导航">
      <div className="sidebar__brand">
        <span className="sidebar__signal" aria-hidden="true" />
        <p>COLDSTART / 冷启动</p>
      </div>

      <div className="sidebar__section-label">WORKSPACE / 阶段导航</div>
      <nav className="sidebar__nav" aria-label="主导航">
        {workspaceItems.map((item) => {
          const enabled = item.view === 'overview' || hasProject;
          const active = item.view === view;
          return (
            <button
              className={`nav-item${active ? ' nav-item--active' : ''}`}
              type="button"
              aria-label={item.label}
              aria-current={active ? 'page' : undefined}
              disabled={!enabled}
              key={item.view}
              onClick={() => onNavigate(item.view)}
            >
              <span className="nav-item__index">{item.index}</span>
              <span>{item.label}</span>
            </button>
          );
        })}

      </nav>

      <div className="sidebar__footer">
        <span className="sidebar__pulse" aria-hidden="true" />
        <div>
          <strong>人工只在必要处介入</strong>
          <span>来源、草稿与决定都留在本地</span>
        </div>
      </div>
    </aside>
  );
}
