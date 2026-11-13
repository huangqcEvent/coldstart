import { useEffect, useState } from 'react';

import { applyTheme, readStoredTheme, type Theme } from '../theme';

function ThemeIcon({ theme }: { theme: Theme }) {
  if (theme === 'dark') {
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
        <path d="M20.5 14.3A8.5 8.5 0 0 1 9.7 3.5 8.5 8.5 0 1 0 20.5 14.3Z" />
      </svg>
    );
  }

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
      <circle cx="12" cy="12" r="3.5" />
      <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
    </svg>
  );
}

export function ThemeToggle() {
  const [theme, setTheme] = useState<Theme>(readStoredTheme);
  const nextTheme = theme === 'dark' ? 'light' : 'dark';
  const actionLabel = `切换到${nextTheme === 'light' ? '浅色' : '深色'}模式`;

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  return (
    <button
      className="theme-toggle"
      type="button"
      aria-label={actionLabel}
      title={actionLabel}
      onClick={() => setTheme(nextTheme)}
    >
      <span className="theme-toggle__icon">
        <ThemeIcon theme={theme} />
      </span>
      <span className="theme-toggle__label">
        {theme === 'dark' ? '深色' : '浅色'}
      </span>
    </button>
  );
}
