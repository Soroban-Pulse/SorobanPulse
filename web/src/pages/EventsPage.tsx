import { useState } from "react";
import { useEvents, type EventType } from "../api/hooks.ts";

/**
 * Full paginated events explorer with type filter.
 */
export default function EventsPage() {
  const [page, setPage] = useState(1);
  const [eventType, setEventType] = useState<EventType | undefined>();
  const limit = 25;

  const { data, isLoading, isFetching, isError } = useEvents(
    { page, limit, event_type: eventType ?? null },
  );

  const totalPages = data ? Math.ceil(data.total / limit) : 0;

  return (
    <div>
      <div className="page-header">
        <h1>Events</h1>
        <p>All indexed Soroban contract events.</p>
      </div>

      {/* Filters */}
      <div className="flex items-center gap-3" style={{ marginBottom: "1rem" }}>
        <label className="flex items-center gap-2">
          <span className="text-muted">Type</span>
          <select
            value={eventType ?? ""}
            onChange={(e) => {
              setEventType((e.target.value as EventType) || undefined);
              setPage(1);
            }}
          >
            <option value="">All</option>
            <option value="contract">contract</option>
            <option value="diagnostic">diagnostic</option>
            <option value="system">system</option>
          </select>
        </label>
      </div>

      <div className="card">
        {isLoading && <p className="text-muted">Loading events…</p>}
        {isError && <div className="error-banner">⚠ Failed to load events</div>}

        {data && (
          <>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>ID</th>
                    <th>Contract</th>
                    <th>Type</th>
                    <th>Ledger</th>
                    <th>Tx Hash</th>
                    <th>Timestamp</th>
                  </tr>
                </thead>
                <tbody>
                  {data.data.length === 0 ? (
                    <tr>
                      <td colSpan={6} style={{ textAlign: "center", padding: "2rem" }}>
                        <span className="text-muted">No events found.</span>
                      </td>
                    </tr>
                  ) : (
                    data.data.map((e) => (
                      <tr key={e.id} style={{ opacity: isFetching ? 0.6 : 1 }}>
                        <td className="mono" title={e.id}>{e.id.slice(0, 10)}…</td>
                        <td className="mono" title={e.contract_id}>{e.contract_id.slice(0, 14)}…</td>
                        <td>
                          <span className={`badge badge--${e.event_type}`}>{e.event_type}</span>
                        </td>
                        <td>{e.ledger.toLocaleString()}</td>
                        <td className="mono" title={e.tx_hash}>{e.tx_hash.slice(0, 10)}…</td>
                        <td className="text-muted">{new Date(e.timestamp).toLocaleString()}</td>
                      </tr>
                    ))
                  )}
                </tbody>
              </table>
            </div>

            <div className="pagination">
              <span className="pagination__info">
                {data.total.toLocaleString()} events · page {page}
                {totalPages > 0 ? ` of ${totalPages}` : ""}
              </span>
              <button
                className="btn"
                onClick={() => setPage((p) => Math.max(1, p - 1))}
                disabled={page === 1}
              >
                ← Prev
              </button>
              <button
                className="btn"
                onClick={() => setPage((p) => p + 1)}
                disabled={page >= totalPages || !data.next_cursor}
              >
                Next →
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
