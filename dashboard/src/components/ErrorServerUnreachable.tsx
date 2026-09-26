import { IconServer } from "./Icons";
import { ErrorState } from "./ErrorState";

export function ErrorServerUnreachable({ onRetry }: { onRetry?: () => void }) {
  return (
    <ErrorState
      icon={<IconServer />}
      title="Server unreachable"
      description="Unable to connect to the SorobanPulse server. Check your network connection and try again."
      retryLabel={onRetry ? "Retry" : undefined}
      onRetry={onRetry}
    />
  );
}
