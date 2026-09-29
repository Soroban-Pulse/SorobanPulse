import { subscriptionsApi } from '../../api/subscriptions';
import type { Delivery } from '../../api/types';
import { AsyncBoundary } from '../../components/AsyncBoundary';
import { Modal } from '../../components/Modal';
import { HttpStatus, StatusBadge } from '../../components/StatusBadge';
import { formatDate } from '../../lib/format';
import { useAsync } from '../../lib/useAsync';
import { MAX_BODY_CHARS, prepareBody, redactHeaders } from './redact';

interface Props {
  subscriptionId: string;
  delivery: Delivery | null;
  onClose: () => void;
  onRedeliver: (d: Delivery) => void;
}

function Headers({ headers }: { headers: Record<string, string> | undefined }) {
  const rows = redactHeaders(headers);
  if (rows.length === 0) return <p className="muted">No headers recorded.</p>;
  return (
    <dl className="kv">
      {rows.map(([k, v]) => (
        <div key={k}>
          <dt className="mono">{k}</dt>
          <dd className="mono">{v}</dd>
        </div>
      ))}
    </dl>
  );
}

function Body({ body, truncated }: { body: string | undefined; truncated?: boolean }) {
  const p = prepareBody(body, truncated);
  if (!p.text) return <p className="muted">Empty body.</p>;
  return (
    <>
      <pre className="code-block">{p.text}</pre>
      {p.truncated && <p className="help">Truncated to {MAX_BODY_CHARS.toLocaleString()} characters.</p>}
    </>
  );
}

export function DeliveryDetailModal({ subscriptionId, delivery, onClose, onRedeliver }: Props) {
  const detail = useAsync(
    () => (delivery ? subscriptionsApi.delivery(subscriptionId, delivery.id) : Promise.resolve(null)),
    [subscriptionId, delivery?.id],
  );
  const d = detail.data;

  return (
    <Modal
      title="Delivery details"
      open={delivery !== null}
      onClose={onClose}
      wide
      footer={
        delivery && (
          <>
            <button type="button" className="btn" onClick={onClose}>
              Close
            </button>
            <button
              type="button"
              className="btn btn-primary"
              onClick={() => onRedeliver(delivery)}
              disabled={delivery.status === 'pending'}
            >
              Redeliver
            </button>
          </>
        )
      }
    >
      {delivery && (
        <div className="stack">
          <div className="detail-summary">
            <StatusBadge status={delivery.status} />
            <HttpStatus code={delivery.status_code} />
            <span>attempt {delivery.attempt}</span>
            <span>{delivery.latency_ms != null ? `${delivery.latency_ms} ms` : '—'}</span>
            <span className="muted">{formatDate(delivery.attempted_at)}</span>
          </div>
          {delivery.error && <p className="alert alert-error">{delivery.error}</p>}
          <AsyncBoundary loading={detail.loading} error={detail.error} onRetry={detail.reload}>
            {d && (
              <div className="detail-grid">
                <section>
                  <h3>Request</h3>
                  <p className="mono small">POST {d.request.url}</p>
                  <h4>Headers</h4>
                  <Headers headers={d.request.headers} />
                  <h4>Body</h4>
                  <Body body={d.request.body} truncated={d.request.truncated} />
                </section>
                <section>
                  <h3>Response</h3>
                  {d.response ? (
                    <>
                      <h4>Headers</h4>
                      <Headers headers={d.response.headers} />
                      <h4>Body</h4>
                      <Body body={d.response.body} truncated={d.response.truncated} />
                    </>
                  ) : (
                    <p className="muted">No response received (timeout or connection error).</p>
                  )}
                </section>
              </div>
            )}
          </AsyncBoundary>
        </div>
      )}
    </Modal>
  );
}
