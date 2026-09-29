import { IconBolt } from "./Icons";
import { ErrorState } from "./ErrorState";

interface ErrorRateLimitedProps {
  retryAfter?: number | null;
  onRetry?: () => void;
}

export function ErrorRateLimited({ retryAfter, onRetry }: ErrorRateLimitedProps) {
  const retryText = retryAfter != null && retryAfter > 0
    ? `Please wait ${retryAfter} second${retryAfter !== 1 ? "s" : ""} before retrying.`
    : "Please wait a moment before retrying.";

  return (
    <ErrorState
      icon={<IconBolt />}
      title="Rate limited"
      description={`You are making requests too quickly. ${retryText}`}
      retryLabel={onRetry ? "Retry now" : undefined}
      onRetry={onRetry}
    />
  );
}
