import { IconError } from "./Icons";

interface ErrorStateProps {
  icon?: React.ReactNode;
  title: string;
  description: string;
  retryLabel?: string;
  onRetry?: () => void;
}

export function ErrorState({
  icon,
  title,
  description,
  retryLabel,
  onRetry,
}: ErrorStateProps) {
  return (
    <div className="error-state">
      <div className="error-state-icon">{icon ?? <IconError />}</div>
      <h3 className="error-state-title">{title}</h3>
      <p className="error-state-description">{description}</p>
      {retryLabel && onRetry && (
        <button className="btn btn-primary" onClick={onRetry}>
          {retryLabel}
        </button>
      )}
    </div>
  );
}
