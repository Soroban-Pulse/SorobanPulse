import { formatNumber } from '../../lib/format';

interface BarListProps {
  items: { label: string; value: number }[];
  /** Fold anything past this many rows into "Other" rather than inventing more rows. */
  maxItems?: number;
  ariaLabel: string;
}

/**
 * Horizontal ranked bars for magnitude comparison. One hue (it is a single
 * measure), labels and values in text ink beside the bar, not on it.
 */
export function BarList({ items, maxItems = 8, ariaLabel }: BarListProps) {
  const sorted = [...items].sort((a, b) => b.value - a.value);
  let rows = sorted;
  if (sorted.length > maxItems) {
    const head = sorted.slice(0, maxItems - 1);
    const other = sorted.slice(maxItems - 1).reduce((a, b) => a + b.value, 0);
    rows = [...head, { label: 'Other', value: other }];
  }
  const max = Math.max(1, ...rows.map((r) => r.value));
  const total = rows.reduce((a, b) => a + b.value, 0);

  if (rows.length === 0 || total === 0) return <div className="empty">No events recorded yet.</div>;

  return (
    <ul className="barlist" aria-label={ariaLabel}>
      {rows.map((r) => {
        const pct = total ? ((r.value / total) * 100).toFixed(1) : '0';
        return (
          <li key={r.label} className="barlist-row" title={`${r.label}: ${formatNumber(r.value)} (${pct}%)`}>
            <span className="barlist-label">{r.label}</span>
            <span className="barlist-track">
              <span className="barlist-bar" style={{ width: `${(r.value / max) * 100}%` }} />
            </span>
            <span className="barlist-value num">
              {formatNumber(r.value)} <span className="muted">{pct}%</span>
            </span>
          </li>
        );
      })}
    </ul>
  );
}
