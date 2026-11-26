import { IconRetry, IconWarning } from "./Icons";

export interface ErrorBannerProps {
  /** Short summary, e.g. "Couldn't load events." */
  title: string;
  /** Optional detail, e.g. the status code or next step. */
  message?: string;
  /** When provided, a Retry button is shown. */
  onRetry?: () => void;
  retryLabel?: string;
  /** When provided, a dismiss button is shown. */
  onDismiss?: () => void;
}

export function ErrorBanner({
  title,
  message,
  onRetry,
  retryLabel = "Retry",
  onDismiss,
}: ErrorBannerProps) {
  return (
    <div className="error-banner" role="alert">
      <span className="error-banner-icon" aria-hidden="true">
        <IconWarning />
      </span>
      <div className="error-banner-text">
        <strong>{title}</strong>
        {message && <span> {message}</span>}
      </div>
      <div className="error-banner-actions">
        {onRetry && (
          <button type="button" className="btn btn-secondary" onClick={onRetry}>
            <IconRetry /> {retryLabel}
          </button>
        )}
        {onDismiss && (
          <button type="button" className="btn btn-ghost" aria-label="Dismiss" onClick={onDismiss}>
            ×
          </button>
        )}
      </div>
    </div>
  );
}
