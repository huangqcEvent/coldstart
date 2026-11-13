/**
 * 主题类型、持久化键与初始值解析。
 *
 * 主题仅用于本地界面展示；读取失败时回退到深色主题，不修改项目数据。
 */
export type Theme = 'dark' | 'light';

const themeStorageKey = 'coldstart.theme';

export function readStoredTheme(): Theme {
  try {
    return window.localStorage.getItem(themeStorageKey) === 'light'
      ? 'light'
      : 'dark';
  } catch {
    return 'dark';
  }
}

export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme;
  try {
    window.localStorage.setItem(themeStorageKey, theme);
  } catch {}
}

export function initializeTheme(): void {
  const theme = readStoredTheme();
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme;
}
