import { IconSubscriptions } from "./Icons";
import { EmptyState } from "./EmptyState";

interface EmptySubscriptionsProps {
  onCreateSubscription?: () => void;
}

export function EmptySubscriptions({ onCreateSubscription }: EmptySubscriptionsProps) {
  return (
    <EmptyState
      icon={<IconSubscriptions />}
      title="No subscriptions"
      description="You haven't created any subscriptions yet. Create one to start receiving webhook notifications for events."
      actionLabel={onCreateSubscription ? "Create subscription" : undefined}
      onAction={onCreateSubscription}
    />
  );
}
