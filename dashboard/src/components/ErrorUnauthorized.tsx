import { IconLock } from "./Icons";
import { ErrorState } from "./ErrorState";

export function ErrorUnauthorized({ onRetry }: { onRetry?: () => void }) {
  return (
    <ErrorState
      icon={<IconLock />}
      title="Unauthorized"
      description="You don't have permission to access this resource. Please sign in with an account that has the required permissions."
      retryLabel={onRetry ? "Sign out and sign in again" : undefined}
      onRetry={onRetry}
    />
  );
}
