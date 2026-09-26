import { useState } from 'react';
import { contractsApi } from '../../api/contracts';
import type { EventRow } from '../../api/types';
import { AsyncBoundary } from '../../components/AsyncBoundary';
import { CopyButton } from '../../components/CopyButton';
import { formatDate, formatNumber, formatRelative, truncateMiddle } from '../../lib/format';
import { useAsync } from '../../lib/useAsync';

const PAGE_SIZE = 25;

function preview(data: unknown): string {
  const s = typeof data === 'string' ? data : JSON.stringify(data);
  return s && s.length > 120 ? `${s.slice(0, 120)}…` : s ?? '';
}

/** Event explorer table, pre-filtered to one contract. */
export function ContractEventsTable({ contractId }: { contractId: string }) {
  const [page, setPage] = useState(1);
  const [eventType, setEventType] = useState('');
  const [expanded, setExpanded] = useState<EventRow | null>(null);

  const events = useAsync(
    () => contractsApi.events(contractId, { page, limit: PAGE_SIZE, event_type: eventType || undefined }),
    [contractId, page, eventType],
  );
  const list = events.data?.data ?? [];
  const total = events.data?.total;

  return (
    <section className="panel">
      <header className="panel-header">
        <h2>Events</h2>
        <select
          value={eventType}
          onChange={(e) => {
            setEventType(e.target.value);
            setPage(1);
          }}
          aria-label="Event type"
        >
          <option value="">All types</option>
          <option value="contract">contract</option>
          <option value="diagnostic">diagnostic</option>
          <option value="system">system</option>
        </select>
      </header>
      <AsyncBoundary
        loading={events.loading && !events.data}
        error={events.error}
        onRetry={events.reload}
        empty={list.length === 0}
        emptyMessage="No events for this contract yet."
      >
        <div className="table-wrap">
          <table className="data-table">
            <thead>
              <tr>
                <th className="num">Ledger</th>
                <th>Type</th>
                <th>Transaction</th>
                <th>Time</th>
                <th>Data</th>
              </tr>
            </thead>
            <tbody>
              {list.map((e) => (
                <tr key={e.id} className="clickable" onClick={() => setExpanded(expanded?.id === e.id ? null : e)}>
                  <td className="num mono">{formatNumber(e.ledger)}</td>
                  <td>
                    <span className="chip">{e.event_type}</span>
                  </td>
                  <td className="mono small" title={e.tx_hash}>
                    {truncateMiddle(e.tx_hash, 6)}
                  </td>
                  <td title={formatDate(e.timestamp)}>{formatRelative(e.timestamp)}</td>
                  <td className="mono small truncate">{preview(e.event_data)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {expanded && (
          <div className="event-detail">
            <div className="event-detail-head">
              <span className="mono small">tx {expanded.tx_hash}</span>
              <CopyButton value={JSON.stringify(expanded.event_data, null, 2)} label="Copy JSON" />
            </div>
            <pre className="code-block">{JSON.stringify(expanded.event_data, null, 2)}</pre>
          </div>
        )}
        <div className="pager">
          <button type="button" className="btn btn-small" disabled={page <= 1} onClick={() => setPage((p) => p - 1)}>
            Previous
          </button>
          <span className="muted small">
            Page {page}
            {total !== undefined ? ` of ${events.data?.approximate ? '~' : ''}${formatNumber(Math.max(1, Math.ceil(total / PAGE_SIZE)))}` : ''}
          </span>
          <button
            type="button"
            className="btn btn-small"
            disabled={list.length < PAGE_SIZE}
            onClick={() => setPage((p) => p + 1)}
          >
            Next
          </button>
        </div>
      </AsyncBoundary>
    </section>
  );
}
