import { IconError } from "./Icons";
import { ErrorState } from "./ErrorState";

interface Error5xxProps {
  requestId: string | null;
  statusCode?: number;
  onRetry?: () => void;
}

export function Error5xx({ requestId, statusCode = 500, onRetry }: Error5xxProps) {
  return (
    <ErrorState
      icon={<IconError />}
      title={`Server error (${statusCode})`}
      description={
        requestId
          ? `Something went wrong on our end. Please reference request ID ${requestId} when reporting this issue.`
          : "Something went wrong on our end. Please try again later."
      }
      retryLabel={onRetry ? "Retry" : undefined}
      onRetry={onRetry}
    />
  );
}
