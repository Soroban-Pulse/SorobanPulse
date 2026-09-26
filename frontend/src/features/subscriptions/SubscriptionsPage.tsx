import { useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { errorMessage } from '../../api/client';
import { subscriptionsApi } from '../../api/subscriptions';
import type { Subscription } from '../../api/types';
import { AsyncBoundary } from '../../components/AsyncBoundary';
import { ConfirmDialog } from '../../components/ConfirmDialog';
import { HttpStatus, StatusBadge } from '../../components/StatusBadge';
import { useToast } from '../../components/Toast';
import { formatRelative, maskUrl, truncateMiddle } from '../../lib/format';
import { useAsync, useDebounced } from '../../lib/useAsync';
import { SecretReveal } from './SecretReveal';
import { SubscriptionForm } from './SubscriptionForm';

type Pending = { kind: 'pause' | 'resume' | 'delete'; sub: Subscription } | null;

export function SubscriptionsPage() {
  const toast = useToast();
  const navigate = useNavigate();
  const [status, setStatus] = useState('');
  const [contract, setContract] = useState('');
  const [q, setQ] = useState('');
  const [form, setForm] = useState<Subscription | 'new' | null>(null);
  const [secret, setSecret] = useState<string | null>(null);
  const [pending, setPending] = useState<Pending>(null);
  const [testing, setTesting] = useState<string | null>(null);

  const dq = useDebounced(q);
  const dContract = useDebounced(contract);
  const subs = useAsync(
    () => subscriptionsApi.list({ status: status || undefined, contract_id: dContract || undefined, q: dq || undefined, page_size: 100 }),
    [status, dContract, dq],
  );
  const list = subs.data?.data ?? [];

  const sendTest = async (s: Subscription) => {
    setTesting(s.id);
    try {
      await subscriptionsApi.sendTest(s.id);
      toast('Test event queued — opening the delivery log', 'ok');
      navigate(`/subscriptions/${s.id}?watch=1`);
    } catch (e) {
      toast(`Test failed: ${errorMessage(e)}`, 'bad');
    } finally {
      setTesting(null);
    }
  };

  const confirmCopy: Record<'pause' | 'resume' | 'delete', { title: string; message: string; label: string }> = {
    pause: {
      title: 'Pause subscription',
      message: 'Deliveries stop until you resume. Events keep queuing and are delivered in order on resume.',
      label: 'Pause',
    },
    resume: { title: 'Resume subscription', message: 'Queued events will start delivering immediately.', label: 'Resume' },
    delete: {
      title: 'Delete subscription',
      message: 'The subscription is cancelled and its pending deliveries are discarded. This cannot be undone.',
      label: 'Delete subscription',
    },
  };

  return (
    <div className="page">
      <header className="page-header">
        <div>
          <h1>Webhook subscriptions</h1>
          <p className="muted">Push indexed events to your own endpoints.</p>
        </div>
        <button type="button" className="btn btn-primary" onClick={() => setForm('new')}>
          New subscription
        </button>
      </header>

      <div className="filters" role="search">
        <input type="search" placeholder="Search callback URL" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Search subscriptions" />
        <input placeholder="Contract ID" value={contract} onChange={(e) => setContract(e.target.value)} aria-label="Filter by contract" spellCheck={false} />
        <select value={status} onChange={(e) => setStatus(e.target.value)} aria-label="Status">
          <option value="">Any status</option>
          <option value="active">Active</option>
          <option value="paused">Paused</option>
          <option value="cancelled">Cancelled</option>
        </select>
      </div>

      <section className="panel">
        <AsyncBoundary
          loading={subs.loading && !subs.data}
          error={subs.error}
          onRetry={subs.reload}
          empty={list.length === 0}
          emptyMessage="No subscriptions yet."
        >
          <div className="table-wrap">
            <table className="data-table">
              <thead>
                <tr>
                  <th>Target</th>
                  <th>Status</th>
                  <th>Filters</th>
                  <th>Mode</th>
                  <th>Last delivery</th>
                  <th className="actions-col">Actions</th>
                </tr>
              </thead>
              <tbody>
                {list.map((s) => (
                  <tr key={s.id}>
                    <td>
                      <Link to={`/subscriptions/${s.id}`} className="mono">
                        {maskUrl(s.callback_url)}
                      </Link>
                      <div className="muted small">
                        {truncateMiddle(s.id, 4)} · created {formatRelative(s.created_at)}
                      </div>
                    </td>
                    <td>
                      <StatusBadge status={s.status} />
                    </td>
                    <td className="small">
                      {s.contract_ids?.length ? `${s.contract_ids.length} contract${s.contract_ids.length > 1 ? 's' : ''}` : 'all contracts'}
                      <div className="muted">{s.event_types?.length ? s.event_types.join(', ') : 'all event types'}</div>
                    </td>
                    <td className="small">
                      {s.subscription_type === 'batch' ? `batch ≤${s.batch_size} / ${s.batch_timeout_ms}ms` : 'per event'}
                    </td>
                    <td>
                      {s.last_delivery ? (
                        <span>
                          <StatusBadge status={s.last_delivery.status} /> <HttpStatus code={s.last_delivery.status_code} />
                          <div className="muted small">{formatRelative(s.last_delivery.attempted_at)}</div>
                        </span>
                      ) : (
                        <span className="muted">never</span>
                      )}
                    </td>
                    <td className="actions-col">
                      <div className="row-actions">
                        <button type="button" className="btn btn-small" onClick={() => sendTest(s)} disabled={testing === s.id || s.status === 'cancelled'}>
                          {testing === s.id ? 'Sending…' : 'Send test'}
                        </button>
                        {s.status === 'paused' ? (
                          <button type="button" className="btn btn-small" onClick={() => setPending({ kind: 'resume', sub: s })}>
                            Resume
                          </button>
                        ) : (
                          <button type="button" className="btn btn-small" onClick={() => setPending({ kind: 'pause', sub: s })} disabled={s.status !== 'active'}>
                            Pause
                          </button>
                        )}
                        <button type="button" className="btn btn-small btn-ghost" onClick={() => setForm(s)} disabled={s.status === 'cancelled'}>
                          Edit
                        </button>
                        <button type="button" className="btn btn-small btn-ghost danger-text" onClick={() => setPending({ kind: 'delete', sub: s })}>
                          Delete
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </AsyncBoundary>
      </section>

      <SubscriptionForm
        target={form}
        onClose={() => setForm(null)}
        onSaved={(sub, newSecret) => {
          setForm(null);
          subs.reload();
          toast(newSecret ? 'Subscription created' : 'Subscription updated', 'ok');
          if (newSecret) setSecret(newSecret);
          else if (form === 'new') navigate(`/subscriptions/${sub.id}`);
        }}
      />

      <SecretReveal secret={secret} onClose={() => setSecret(null)} />

      {pending && (
        <ConfirmDialog
          open
          title={confirmCopy[pending.kind].title}
          message={
            <>
              <p>{confirmCopy[pending.kind].message}</p>
              <p className="mono small">{maskUrl(pending.sub.callback_url)}</p>
            </>
          }
          confirmLabel={confirmCopy[pending.kind].label}
          danger={pending.kind === 'delete'}
          onClose={() => setPending(null)}
          onConfirm={async () => {
            const { kind, sub } = pending;
            if (kind === 'pause') await subscriptionsApi.pause(sub.id);
            if (kind === 'resume') await subscriptionsApi.resume(sub.id);
            if (kind === 'delete') await subscriptionsApi.remove(sub.id);
            toast(`Subscription ${kind === 'delete' ? 'deleted' : kind === 'pause' ? 'paused' : 'resumed'}`, 'ok');
            subs.reload();
          }}
        />
      )}
    </div>
  );
}
