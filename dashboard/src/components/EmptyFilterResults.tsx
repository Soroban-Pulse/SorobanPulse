import { IconFilter } from "./Icons";
import { EmptyState } from "./EmptyState";

interface EmptyFilterResultsProps {
  onClearFilter?: () => void;
}

export function EmptyFilterResults({ onClearFilter }: EmptyFilterResultsProps) {
  return (
    <EmptyState
      icon={<IconFilter />}
      title="No results"
      description="No events match the current filter criteria. Try adjusting your search or date range."
      actionLabel={onClearFilter ? "Clear filter" : undefined}
      onAction={onClearFilter}
    />
  );
}
