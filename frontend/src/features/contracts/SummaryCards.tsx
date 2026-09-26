import type { ContractSummary } from '../../api/types';
import { formatDate, formatNumber, formatRelative } from '../../lib/format';

export function SummaryCards({ summary }: { summary: ContractSummary }) {
  const b = summary.event_type_breakdown;
  const distinctTypes = [b.contract, b.diagnostic, b.system].filter((n) => n > 0).length;

  const cards = [
    { label: 'Total events', value: formatNumber(summary.total_events), sub: `${formatNumber(summary.unique_tx_count)} transactions` },
    {
      label: 'First seen',
      value: summary.ledger_range.min !== null ? `#${formatNumber(summary.ledger_range.min)}` : '—',
      sub: formatDate(summary.first_event_at),
    },
    {
      label: 'Last seen',
      value: summary.ledger_range.max !== null ? `#${formatNumber(summary.ledger_range.max)}` : '—',
      sub: formatRelative(summary.last_event_at),
    },
    { label: 'Event types', value: String(distinctTypes), sub: 'distinct types emitted' },
  ];

  return (
    <div className="stat-grid">
      {cards.map((c) => (
        <div key={c.label} className="stat-tile">
          <div className="stat-label">{c.label}</div>
          <div className="stat-value">{c.value}</div>
          <div className="stat-sub muted">{c.sub}</div>
        </div>
      ))}
    </div>
  );
}
