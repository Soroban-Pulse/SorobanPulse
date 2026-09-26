import { useState } from 'react';
import { errorMessage } from '../../api/client';
import { channelsApi } from '../../api/channels';
import type { MaintenanceWindow } from '../../api/types';
import { AsyncBoundary } from '../../components/AsyncBoundary';
import { ConfirmDialog } from '../../components/ConfirmDialog';
import { StatusBadge } from '../../components/StatusBadge';
import { formatDate, parseList, truncateMiddle } from '../../lib/format';
import { useAsync } from '../../lib/useAsync';

function windowState(w: MaintenanceWindow): 'active' | 'pending' {
  const now = Date.now();
  return new Date(w.start_time).getTime() <= now && now <= new Date(w.end_time).getTime() ? 'active' : 'pending';
}

/** Maintenance windows suppress notifications (all contracts, or listed ones) while active. */
export function MaintenanceWindowsPanel() {
  const windows = useAsync(() => channelsApi.maintenanceWindows(), []);
  const [adding, setAdding] = useState(false);
  const [start, setStart] = useState('');
  const [end, setEnd] = useState('');
  const [contracts, setContracts] = useState('');
  const [description, setDescription] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<MaintenanceWindow | null>(null);

  const create = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    if (!start || !end) return setError('Start and end are required');
    if (new Date(end) <= new Date(start)) return setError('end_time must be after start_time');
    try {
      await channelsApi.createMaintenanceWindow({
        start_time: new Date(start).toISOString(),
        end_time: new Date(end).toISOString(),
        contract_ids: parseList(contracts),
        description: description || undefined,
      });
      setAdding(false);
      setStart('');
      setEnd('');
      setContracts('');
      setDescription('');
      windows.reload();
    } catch (err) {
      setError(errorMessage(err));
    }
  };

  const list = windows.data?.data ?? [];

  return (
    <section className="panel">
      <header className="panel-header">
        <h2>Maintenance windows</h2>
        <button type="button" className="btn btn-small" onClick={() => setAdding((v) => !v)}>
          {adding ? 'Cancel' : 'Schedule'}
        </button>
      </header>

      {adding && (
        <form className="form-grid compact" onSubmit={create}>
          <label className="field">
            <span>Starts</span>
            <input type="datetime-local" value={start} onChange={(e) => setStart(e.target.value)} />
          </label>
          <label className="field">
            <span>Ends</span>
            <input type="datetime-local" value={end} onChange={(e) => setEnd(e.target.value)} />
          </label>
          <label className="field">
            <span>Contracts (empty = all)</span>
            <input value={contracts} onChange={(e) => setContracts(e.target.value)} spellCheck={false} />
          </label>
          <label className="field">
            <span>Description</span>
            <input value={description} onChange={(e) => setDescription(e.target.value)} />
          </label>
          {error && <p className="alert alert-error">{error}</p>}
          <button type="submit" className="btn btn-primary">
            Save window
          </button>
        </form>
      )}

      <AsyncBoundary
        loading={windows.loading}
        error={windows.error}
        onRetry={windows.reload}
        empty={list.length === 0}
        emptyMessage="No current or upcoming maintenance windows."
      >
        <ul className="window-list">
          {list.map((w) => (
            <li key={w.id} className="window-row">
              <StatusBadge status={windowState(w)} label={windowState(w) === 'active' ? 'in progress' : 'scheduled'} />
              <div className="window-body">
                <div>
                  {formatDate(w.start_time)} → {formatDate(w.end_time)}
                </div>
                <div className="muted">
                  {w.description || 'No description'} ·{' '}
                  {w.contract_ids.length ? w.contract_ids.map((c) => truncateMiddle(c, 4)).join(', ') : 'all contracts'}
                </div>
              </div>
              <button type="button" className="btn btn-small btn-ghost" onClick={() => setDeleting(w)}>
                Delete
              </button>
            </li>
          ))}
        </ul>
      </AsyncBoundary>

      <ConfirmDialog
        open={deleting !== null}
        title="Delete maintenance window"
        message="Notifications will resume immediately for the affected contracts."
        confirmLabel="Delete"
        danger
        onClose={() => setDeleting(null)}
        onConfirm={async () => {
          if (deleting) await channelsApi.deleteMaintenanceWindow(deleting.id);
          windows.reload();
        }}
      />
    </section>
  );
}
