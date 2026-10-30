import type { CommandError } from '../api/types';

interface ErrorBannerProps {
  error: CommandError;
  onDismiss: () => void;
}

export function ErrorBanner({ error, onDismiss }: ErrorBannerProps) {
  return (
    <section className="error-banner" role="alert" aria-live="assertive">
      <div className="error-banner__indicator" aria-hidden="true">
        !
      </div>
      <div className="error-banner__body">
        <p>
          <span>错误码</span>
          <code>{error.code}</code>
        </p>
        <p>{error.message}</p>
      </div>
      <button
        className="error-banner__close"
        type="button"
        aria-label="关闭错误提示"
        onClick={onDismiss}
      >
        <span aria-hidden="true">×</span>
      </button>
    </section>
  );
}
