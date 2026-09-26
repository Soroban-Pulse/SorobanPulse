import type { ReactNode } from 'react';
import { errorMessage } from '../api/client';

interface Props {
  loading: boolean;
  error: unknown;
  onRetry?: () => void;
  empty?: boolean;
  emptyMessage?: ReactNode;
  children: ReactNode;
}

/** Standard loading / error / empty states for a data-backed section. */
export function AsyncBoundary({ loading, error, onRetry, empty, emptyMessage, children }: Props) {
  if (error) {
    return (
      <div className="alert alert-error" role="alert">
        <span>{errorMessage(error)}</span>
        {onRetry && (
          <button type="button" className="btn btn-small" onClick={onRetry}>
            Retry
          </button>
        )}
      </div>
    );
  }
  if (loading) return <div className="skeleton" aria-busy="true" aria-label="Loading" />;
  if (empty) return <div className="empty">{emptyMessage ?? 'Nothing here yet.'}</div>;
  return <>{children}</>;
}
