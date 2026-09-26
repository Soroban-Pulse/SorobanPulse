import { IconClock } from "./Icons";
import { EmptyState } from "./EmptyState";

interface EmptyEventsProps {
  lagSeconds?: number;
}

export function EmptyEvents({ lagSeconds }: EmptyEventsProps) {
  const lagText = lagSeconds != null
    ? `The indexer is catching up — current lag is ${lagSeconds}s.`
    : "The indexer is still catching up. No events have been indexed yet.";

  return (
    <EmptyState
      icon={<IconClock />}
      title="No events yet"
      description={lagText}
    />
  );
}
