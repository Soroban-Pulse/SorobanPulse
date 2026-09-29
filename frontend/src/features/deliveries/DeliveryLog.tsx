import { useEffect, useState } from 'react';
import { errorMessage } from '../../api/client';
import { subscriptionsApi } from '../../api/subscriptions';
import type { Delivery } from '../../api/types';
import { AsyncBoundary } from '../../components/AsyncBoundary';
import { HttpStatus, StatusBadge } from '../../components/StatusBadge';
import { useToast } from '../../components/Toast';
import { formatDate, formatRelative } from '../../lib/format';
import { useAsync } from '../../lib/useAsync';
import { DeliveryDetailModal } from './DeliveryDetailModal';

const PAGE_SIZE = 50;

interface Props {
  subscriptionId: string;
  /** Poll for new deliveries (e.g. right after sending a test event). */
  watch?: boolean;
}

export function DeliveryLog({ subscriptionId, watch }: Props) {
  const toast = useToast();
  const [status, setStatus] = useState('');
  const [page, setPage] = useState(1);
  const [selected, setSelected] = useState<Delivery | null>(null);
  const [polling, setPolling] = useState(Boolean(watch));

  const deliveries = useAsync(
    () => subscriptionsApi.deliveries(subscriptionId, { status: status || undefined, page, page_size: PAGE_SIZE }),
    [subscriptionId, status, page],
  );

  // After a test send, refresh every 2s for up to 30s so the delivery shows up without manual reloads.
  useEffect(() => {
    if (!polling) return;
    const iv = setInterval(deliveries.reload, 2000);
    const stop = setTimeout(() => setPolling(false), 30_000);
    return () => {
      clearInterval(iv);
      clearTimeout(stop);
    };
  }, [polling, deliveries.reload]);

  const patch = (id: string, changes: Partial<Delivery>) =>
    deliveries.setData((d) => d && { ...d, data: d.data.map((x) => (x.id === id ? { ...x, ...changes } : x)) });

  const redeliver = async (d: Delivery) => {
    // Optimistic: show the delivery as pending straight away, roll back if the request fails.
    patch(d.id, { status: 'pending', error: null });
    setSelected((s) => (s?.id === d.id ? { ...s, status: 'pending', error: null } : s));
    try {
      const result = await subscriptionsApi.redeliver(subscriptionId, d.id);
      if (result && result.id === d.id) patch(d.id, result);
      else if (result) deliveries.setData((prev) => prev && { ...prev, data: [result, ...prev.data] });
      toast('Redelivery queued', 'ok');
      setPolling(true);
    } catch (e) {
      patch(d.id, { status: d.status, error: d.error });
      setSelected((s) => (s?.id === d.id ? d : s));
      toast(`Redeliver failed: ${errorMessage(e)}`, 'bad');
    }
  };

  const list = deliveries.data?.data ?? [];
  const total = deliveries.data?.total;

  return (
    <section className="panel">
      <header className="panel-header">
        <h2>Delivery log</h2>
        <div className="filters inline">
          {polling && <span className="muted small live-dot">watching for new deliveries</span>}
          <div className="segmented" role="group" aria-label="Filter by status">
            {[
              ['', 'All'],
              ['failed', 'Failed'],
              ['success', 'Succeeded'],
              ['pending', 'Pending'],
            ].map(([v, label]) => (
              <button
                key={v}
                type="button"
                aria-pressed={status === v}
                className={status === v ? 'active' : undefined}
                onClick={() => {
                  setStatus(v);
                  setPage(1);
                }}
              >
                {label}
              </button>
            ))}
          </div>
          <button type="button" className="btn btn-small btn-ghost" onClick={deliveries.reload}>
            Refresh
          </button>
        </div>
      </header>

      <AsyncBoundary
        loading={deliveries.loading && !deliveries.data}
        error={deliveries.error}
        onRetry={deliveries.reload}
        empty={list.length === 0}
        emptyMessage={status === 'failed' ? 'No failed deliveries.' : 'No deliveries yet. Send a test event to try the endpoint.'}
      >
        <div className="table-wrap">
          <table className="data-table">
            <thead>
              <tr>
                <th>Status</th>
                <th>HTTP</th>
                <th className="num">Latency</th>
                <th className="num">Attempt</th>
                <th>Time</th>
                <th>Error</th>
                <th className="actions-col">
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {list.map((d) => (
                <tr key={d.id} className="clickable" onClick={() => setSelected(d)}>
                  <td>
                    <StatusBadge status={d.status} />
                  </td>
                  <td>
                    <HttpStatus code={d.status_code} />
                  </td>
                  <td className="num">{d.latency_ms != null ? `${d.latency_ms} ms` : '—'}</td>
                  <td className="num">{d.attempt}</td>
                  <td title={formatDate(d.attempted_at)}>{formatRelative(d.attempted_at)}</td>
                  <td className="truncate small" title={d.error ?? undefined}>
                    {d.error ?? ''}
                  </td>
                  <td className="actions-col">
                    <div className="row-actions">
                      <button type="button" className="btn btn-small btn-ghost" onClick={(e) => { e.stopPropagation(); setSelected(d); }}>
                        Details
                      </button>
                      {d.status !== 'success' && (
                        <button
                          type="button"
                          className="btn btn-small"
                          disabled={d.status === 'pending'}
                          onClick={(e) => {
                            e.stopPropagation();
                            redeliver(d);
                          }}
                        >
                          Redeliver
                        </button>
                      )}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <div className="pager">
          <button type="button" className="btn btn-small" disabled={page <= 1} onClick={() => setPage((p) => p - 1)}>
            Previous
          </button>
          <span className="muted small">
            Page {page}
            {total !== undefined ? ` of ${Math.max(1, Math.ceil(total / PAGE_SIZE))}` : ''}
          </span>
          <button
            type="button"
            className="btn btn-small"
            disabled={total !== undefined ? page * PAGE_SIZE >= total : list.length < PAGE_SIZE}
            onClick={() => setPage((p) => p + 1)}
          >
            Next
          </button>
        </div>
      </AsyncBoundary>

      <DeliveryDetailModal
        subscriptionId={subscriptionId}
        delivery={selected}
        onClose={() => setSelected(null)}
        onRedeliver={redeliver}
      />
    </section>
  );
}
