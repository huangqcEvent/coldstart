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
