import { useState } from 'react';
import { Link, useParams, useSearchParams } from 'react-router-dom';
import { errorMessage } from '../../api/client';
import { subscriptionsApi } from '../../api/subscriptions';
import { AsyncBoundary } from '../../components/AsyncBoundary';
import { StatusBadge } from '../../components/StatusBadge';
import { useToast } from '../../components/Toast';
import { formatDate, formatNumber, maskUrl } from '../../lib/format';
import { useAsync } from '../../lib/useAsync';
import { DeliveryLog } from '../deliveries/DeliveryLog';

export function SubscriptionDetailPage() {
  const { id = '' } = useParams();
  const [params] = useSearchParams();
  const toast = useToast();
  const sub = useAsync(() => subscriptionsApi.get(id), [id]);
  const [watchKey, setWatchKey] = useState(params.get('watch') ? 1 : 0);
  const [testing, setTesting] = useState(false);

  const sendTest = async () => {
    setTesting(true);
    try {
      await subscriptionsApi.sendTest(id);
      toast('Test event queued', 'ok');
      // Remount the log in watch mode so it polls for the new delivery.
      setWatchKey((k) => k + 1);
    } catch (e) {
      toast(`Test failed: ${errorMessage(e)}`, 'bad');
    } finally {
      setTesting(false);
    }
  };

  const s = sub.data;

  return (
    <div className="page">
      <nav className="breadcrumbs">
        <Link to="/subscriptions">Subscriptions</Link> / <span className="mono">{id.slice(0, 8)}</span>
      </nav>

      <AsyncBoundary loading={sub.loading} error={sub.error} onRetry={sub.reload}>
        {s && (
          <header className="page-header">
            <div>
              <h1 className="mono">{maskUrl(s.callback_url)}</h1>
              <div className="meta-row">
                <StatusBadge status={s.status} />
                <span>{s.subscription_type === 'batch' ? `batch ≤${s.batch_size}` : 'per event'}</span>
                <span className="muted">from ledger {formatNumber(s.from_ledger)}</span>
                <span className="muted">acked {formatNumber(s.acked_ledger)}</span>
                <span className="muted">created {formatDate(s.created_at)}</span>
              </div>
            </div>
            <button type="button" className="btn btn-primary" onClick={sendTest} disabled={testing || s.status === 'cancelled'}>
              {testing ? 'Sending…' : 'Send test event'}
            </button>
          </header>
        )}
      </AsyncBoundary>

      <DeliveryLog key={watchKey} subscriptionId={id} watch={watchKey > 0} />
    </div>
  );
}
