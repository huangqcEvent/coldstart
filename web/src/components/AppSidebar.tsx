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

interface NavigationItem {
  view: WorkspaceView;
  label: string;
}

const navigationGroups: Array<{
  label: string;
  items: NavigationItem[];
}> = [
  {
    label: '规划',
    items: [
      { view: 'overview', label: '项目总览' },
      { view: 'requirements', label: '需求导入' },
      { view: 'blueprint', label: '蓝图编排' },
      { view: 'inbox', label: '人工 Inbox' },
    ],
  },
  {
    label: '交付',
    items: [
      { view: 'plan', label: '计划看板' },
      { view: 'integration', label: '仓库与能力' },
      { view: 'review', label: 'AI 评审' },
      { view: 'release', label: '发布中心' },
      { view: 'memory', label: '项目记忆' },
    ],
  },
];

function NavigationIcon({ view }: { view: WorkspaceView }) {
  const commonProps = {
    width: 18,
    height: 18,
    viewBox: '0 0 24 24',
    fill: 'none',
    stroke: 'currentColor',
    strokeWidth: 1.8,
    strokeLinecap: 'round' as const,
    strokeLinejoin: 'round' as const,
    'aria-hidden': true,
  };

  switch (view) {
    case 'overview':
      return (
        <svg {...commonProps}>
          <rect x="3" y="3" width="7" height="7" rx="1.5" />
          <rect x="14" y="3" width="7" height="7" rx="1.5" />
          <rect x="3" y="14" width="7" height="7" rx="1.5" />
          <rect x="14" y="14" width="7" height="7" rx="1.5" />
        </svg>
      );
    case 'requirements':
      return (
        <svg {...commonProps}>
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
          <path d="M14 2v6h6M8 13h8M8 17h6" />
        </svg>
      );
    case 'blueprint':
      return (
        <svg {...commonProps}>
          <path d="M4 5.5A1.5 1.5 0 0 1 5.5 4H10v5H4zM14 4h4.5A1.5 1.5 0 0 1 20 5.5V10h-6zM4 14h6v6H5.5A1.5 1.5 0 0 1 4 18.5zM14 14h6v4.5a1.5 1.5 0 0 1-1.5 1.5H14z" />
        </svg>
      );
    case 'inbox':
      return (
        <svg {...commonProps}>
          <path d="m22 12-4.2 6.3A2 2 0 0 1 16.1 19H7.9a2 2 0 0 1-1.7-.9L2 12" />
          <path d="M5.5 5h13l3.5 7H16l-2 3H10l-2-3H2z" />
        </svg>
      );
    case 'plan':
      return (
        <svg {...commonProps}>
          <rect x="3" y="4" width="18" height="17" rx="2" />
          <path d="M8 2v4M16 2v4M3 9h18M8 13l2 2 4-4M8 19h5" />
        </svg>
      );
    case 'integration':
      return (
        <svg {...commonProps}>
          <circle cx="6" cy="6" r="2.5" />
          <circle cx="18" cy="6" r="2.5" />
          <circle cx="12" cy="18" r="2.5" />
          <path d="M8.5 6h7M7.4 8.2l3.2 7.6M16.6 8.2l-3.2 7.6" />
        </svg>
      );
    case 'review':
      return (
        <svg {...commonProps}>
          <path d="M12 3 4.5 6v5.2c0 4.7 3 8.1 7.5 9.8 4.5-1.7 7.5-5.1 7.5-9.8V6z" />
          <path d="m9 12 2 2 4-4" />
        </svg>
      );
    case 'release':
      return (
        <svg {...commonProps}>
          <path d="M4 15a3 3 0 0 0 5 2.2L13 21l2-2-3.8-4A3 3 0 0 0 13.2 10L20 3.2 13 3l-7 7a3 3 0 0 0-2 5z" />
          <circle cx="15" cy="9" r="1.2" />
          <path d="M7 17c-1.5 1.5-2 4-2 4s2.5-.5 4-2" />
        </svg>
      );
    case 'memory':
      return (
        <svg {...commonProps}>
          <path d="M3 12a9 9 0 1 0 3-6.7L3 8" />
          <path d="M3 3v5h5M12 7v5l3 2" />
        </svg>
      );
  }
}

export function AppSidebar({ view, hasProject, onNavigate }: AppSidebarProps) {
  return (
    <aside className="sidebar" aria-label="应用导航">
      <div className="sidebar__brand">
        <span className="sidebar__mark" aria-hidden="true">
          <img
            className="sidebar__mark-image sidebar__mark-image--on-dark"
            src="/brand/coldstart-mark-on-dark.png"
            alt=""
            draggable={false}
          />
          <img
            className="sidebar__mark-image sidebar__mark-image--on-light"
            src="/brand/coldstart-mark-on-light.png"
            alt=""
            draggable={false}
          />
        </span>
        <div className="sidebar__brand-copy">
          <strong>ColdStart</strong>
          <span>冷启动工作台</span>
        </div>
      </div>

      <nav className="sidebar__nav" aria-label="主导航">
        {navigationGroups.map((group) => (
          <div className="sidebar__group" key={group.label}>
            <p className="sidebar__section-label">{group.label}</p>
            {group.items.map((item) => {
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
                  <span className="nav-item__icon">
                    <NavigationIcon view={item.view} />
                  </span>
                  <span className="nav-item__label">{item.label}</span>
                </button>
              );
            })}
          </div>
        ))}
      </nav>

      <div className="sidebar__footer">
        <span className="sidebar__pulse" aria-hidden="true" />
        <div>
          <strong>本地工作区</strong>
          <span>数据仅保存在此电脑</span>
        </div>
      </div>
    </aside>
  );
}
