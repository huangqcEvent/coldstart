/**
 * 统一错误提示条。
 *
 * 展示后端稳定错误码和可安全公开的消息，由父级决定是否清除或重试。
 */
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
