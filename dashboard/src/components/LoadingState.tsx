import { SkeletonCard } from "./SkeletonCard";
import { SkeletonTable } from "./SkeletonTable";
import { SkeletonChart } from "./SkeletonChart";

export type LoadingLayout = "table" | "cards" | "chart";

export interface LoadingStateProps {
  /** Which layout the skeleton should mimic. */
  layout?: LoadingLayout;
  /** Rows for the table layout, or number of cards for the cards layout. */
  rows?: number;
  /** Columns for the table layout. */
  columns?: number;
  /** Accessible label announced to screen readers. */
  label?: string;
}

export function LoadingState({
  layout = "table",
  rows = 5,
  columns = 4,
  label = "Loading…",
}: LoadingStateProps) {
  return (
    <div className="loading-state" role="status" aria-live="polite" aria-busy="true">
      <span className="sr-only">{label}</span>
      {layout === "table" && <SkeletonTable rows={rows} columns={columns} />}
      {layout === "chart" && <SkeletonChart height="300px" />}
      {layout === "cards" && (
        <div className="loading-state-cards">
          {Array.from({ length: rows }, (_, i) => (
            <SkeletonCard key={i} width="12rem" height="80px" />
          ))}
        </div>
      )}
    </div>
  );
}
