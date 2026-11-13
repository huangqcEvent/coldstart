/**
 * 应用侧边栏项目切换器。
 *
 * 展示当前项目和最近项目，提供切换、打开、新建、移除历史与清空访问记录；
 * 组件只发送用户意图，项目校验和持久化都由 Rust 端完成。
 */
import { useEffect, useLayoutEffect, useRef, useState } from 'react';

import type { ProjectStatus, RecentProjectView } from '../api/types';

interface ProjectSwitcherProps {
  currentProject: ProjectStatus | null;
  projects: RecentProjectView[];
  busy: boolean;
  onSelect: (path: string) => void;
  onOpen: () => void;
  onCreate: () => void;
  onForget: (path: string) => void;
  onClear: () => void;
}

function ProjectIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M3 7.5A1.5 1.5 0 0 1 4.5 6H9l2 2h8.5A1.5 1.5 0 0 1 21 9.5v8a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5z" />
    </svg>
  );
}

function RemoveIcon() {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      aria-hidden="true"
    >
      <path d="m6 6 12 12M18 6 6 18" />
    </svg>
  );
}

export function ProjectSwitcher({
  currentProject,
  projects,
  busy,
  onSelect,
  onOpen,
  onCreate,
  onForget,
  onClear,
}: ProjectSwitcherProps) {
  const [open, setOpen] = useState(false);
  const [confirmingClear, setConfirmingClear] = useState(false);
  const [menuPosition, setMenuPosition] = useState<{
    top: number;
    left: number;
    width: number;
  } | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);

  useLayoutEffect(() => {
    if (!open) {
      setMenuPosition(null);
      return;
    }

    function updatePosition() {
      const trigger = triggerRef.current;
      if (!trigger) {
        return;
      }
      const rect = trigger.getBoundingClientRect();
      const width = Math.min(320, window.innerWidth - 28);
      const left = Math.min(rect.left, window.innerWidth - width - 14);
      setMenuPosition({
        top: rect.bottom + 7,
        left: Math.max(14, left),
        width,
      });
    }

    updatePosition();
    window.addEventListener('resize', updatePosition);
    window.addEventListener('scroll', updatePosition, true);
    return () => {
      window.removeEventListener('resize', updatePosition);
      window.removeEventListener('scroll', updatePosition, true);
    };
  }, [open]);

  useEffect(() => {
    if (!open) {
      setConfirmingClear(false);
      return;
    }

    function handlePointerDown(event: PointerEvent) {
      if (!rootRef.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === 'Escape') {
        setOpen(false);
      }
    }

    window.addEventListener('pointerdown', handlePointerDown);
    window.addEventListener('keydown', handleKeyDown);
    return () => {
      window.removeEventListener('pointerdown', handlePointerDown);
      window.removeEventListener('keydown', handleKeyDown);
    };
  }, [open]);

  const currentName = currentProject?.name ?? '选择项目';

  return (
    <div className="project-switcher" ref={rootRef}>
      <button
        ref={triggerRef}
        className="project-switcher__trigger"
        type="button"
        aria-label={`切换项目：${currentName}`}
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        <span className="project-switcher__icon" aria-hidden="true">
          <ProjectIcon />
        </span>
        <span className="project-switcher__copy">
          <span>{currentProject ? '当前项目' : '项目工作区'}</span>
          <strong>{currentName}</strong>
        </span>
        <span className="project-switcher__chevron" aria-hidden="true">
          {open ? '▴' : '▾'}
        </span>
      </button>

      {open && menuPosition ? (
        <div
          className="project-switcher__menu"
          role="dialog"
          aria-label="切换项目"
          style={{
            top: menuPosition.top,
            left: menuPosition.left,
            width: menuPosition.width,
          }}
        >
          <div className="project-switcher__menu-head">
            <span>最近项目</span>
            <small>最多保留 12 个</small>
          </div>

          {projects.length > 0 ? (
            <div className="project-switcher__list">
              {projects.map((project) => {
                const current = project.rootPath === currentProject?.rootPath;
                return (
                  <div
                    className={`project-switcher__project${current ? ' project-switcher__project--current' : ''}`}
                    key={project.rootPath}
                  >
                    <button
                      className="project-switcher__project-main"
                      type="button"
                      aria-label={`切换到 ${project.name}`}
                      disabled={busy || current || !project.available}
                      onClick={() => {
                        onSelect(project.rootPath);
                        setOpen(false);
                      }}
                    >
                      <span>
                        <strong>{project.name}</strong>
                        <small title={project.rootPath}>{project.rootPath}</small>
                      </span>
                      <em className={project.available ? '' : 'is-unavailable'}>
                        {current ? '当前' : project.available ? '可切换' : '不可用'}
                      </em>
                    </button>
                    <button
                      className="project-switcher__remove"
                      type="button"
                      aria-label={`从历史记录移除 ${project.name}`}
                      title="仅从访问历史移除，不删除项目文件"
                      disabled={busy}
                      onClick={() => {
                        onForget(project.rootPath);
                        setOpen(false);
                      }}
                    >
                      <RemoveIcon />
                    </button>
                  </div>
                );
              })}
            </div>
          ) : (
            <p className="project-switcher__empty">
              还没有最近项目。打开或新建项目后会显示在这里。
            </p>
          )}

          <div className="project-switcher__actions">
            <button
              className="button button--ghost"
              type="button"
              disabled={busy}
              onClick={() => {
                onCreate();
                setOpen(false);
              }}
            >
              新建项目
            </button>
            <button
              className="button button--ghost"
              type="button"
              disabled={busy}
              onClick={() => {
                onOpen();
                setOpen(false);
              }}
            >
              打开其他项目
            </button>
          </div>

          {projects.length > 0 ? (
            <div className="project-switcher__danger">
              <button
                className="button button--quiet"
                type="button"
                disabled={busy}
                onClick={() => {
                  if (!confirmingClear) {
                    setConfirmingClear(true);
                    return;
                  }
                  onClear();
                  setOpen(false);
                }}
              >
                {confirmingClear ? '确认清空访问历史' : '清除访问历史'}
              </button>
              <small>只清访问记录，不删除项目文件</small>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
