import { useEffect, useState } from 'react';
import { errorMessage } from '../../api/client';
import { dlqApi } from '../../api/subscriptions';
import type { DlqFilter, DlqReplayJob } from '../../api/types';
import { AsyncBoundary } from '../../components/AsyncBoundary';
import { ConfirmDialog } from '../../components/ConfirmDialog';
import { HttpStatus } from '../../components/StatusBadge';
import { useToast } from '../../components/Toast';
import { formatNumber, formatRelative, maskUrl } from '../../lib/format';
import { useAsync, useDebounced } from '../../lib/useAsync';

const PAGE_SIZE = 50;

interface FilterForm {
  endpoint_url: string;
  failure_reason: string;
  created_after: string;
  created_before: string;
  max_attempts: string;
}

const EMPTY: FilterForm = { endpoint_url: '', failure_reason: '', created_after: '', created_before: '', max_attempts: '' };

function toFilter(f: FilterForm): DlqFilter {
  return {
    endpoint_url: f.endpoint_url || undefined,
    failure_reason: f.failure_reason || undefined,
    created_after: f.created_after ? new Date(f.created_after).toISOString() : undefined,
    created_before: f.created_before ? new Date(f.created_before).toISOString() : undefined,
    max_attempts: f.max_attempts ? Number(f.max_attempts) : undefined,
  };
}

export function DlqPage() {
  const toast = useToast();
  const [form, setForm] = useState<FilterForm>(EMPTY);
  const [page, setPage] = useState(1);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [preview, setPreview] = useState<number | null>(null);
  const [previewing, setPreviewing] = useState(false);
  const [confirm, setConfirm] = useState<'filter' | 'selected' | null>(null);
  const [job, setJob] = useState<DlqReplayJob | null>(null);

  const debounced = useDebounced(form);
  const filter = toFilter(debounced);
  const filterKey = JSON.stringify(filter);

  const entries = useAsync(() => dlqApi.list({ ...filter, page, page_size: PAGE_SIZE }), [filterKey, page]);

  // A changed filter invalidates the preview count and selection.
  useEffect(() => {
    setPreview(null);
    setSelected(new Set());
    setPage(1);
  }, [filterKey]);

  // Poll the replay job until it finishes.
  useEffect(() => {
    if (!job || job.done) return;
    const iv = setInterval(async () => {
      try {
        const next = await dlqApi.job(job.job_id);
        setJob(next);
        if (next.done) {
          toast(`Replay finished: ${next.succeeded} delivered, ${next.failed} failed`, next.failed ? 'bad' : 'ok');
          entries.reload();
        }
      } catch (e) {
        toast(errorMessage(e), 'bad');
        setJob(null);
      }
    }, 1500);
    return () => clearInterval(iv);
  }, [job, toast, entries.reload]);

  const runPreview = async () => {
    setPreviewing(true);
    try {
      const res = await dlqApi.replay(filter, true);
      setPreview(res.matched);
    } catch (e) {
      toast(errorMessage(e), 'bad');
    } finally {
      setPreviewing(false);
    }
  };

  const startReplay = async (kind: 'filter' | 'selected') => {
    const res = kind === 'filter' ? await dlqApi.replay(filter, false) : await dlqApi.replayIds([...selected]);
    setSelected(new Set());
    if (res.job_id) {
      setJob({ job_id: res.job_id, total: res.matched, processed: 0, succeeded: 0, failed: 0, done: false });
    } else {
      toast(`${formatNumber(res.replayed)} deliveries re-queued`, 'ok');
      entries.reload();
    }
  };

  const list = entries.data?.data ?? [];
  const total = entries.data?.total;
  const allOnPage = list.length > 0 && list.every((e) => selected.has(e.id));
  const set = (k: keyof FilterForm) => (e: React.ChangeEvent<HTMLInputElement>) => setForm((f) => ({ ...f, [k]: e.target.value }));

  return (
    <div className="page">
      <header className="page-header">
        <div>
          <h1>Dead-letter queue</h1>
          <p className="muted">Webhook deliveries that exhausted their retries. Replay re-queues them with a fresh attempt budget.</p>
        </div>
      </header>

      <form className="filters filters-wrap" role="search" onSubmit={(e) => e.preventDefault()}>
        <input placeholder="Endpoint URL contains" value={form.endpoint_url} onChange={set('endpoint_url')} aria-label="Endpoint URL" />
        <input placeholder="Failure reason contains" value={form.failure_reason} onChange={set('failure_reason')} aria-label="Failure reason" />
        <label className="inline-label">
          After <input type="datetime-local" value={form.created_after} onChange={set('created_after')} />
        </label>
        <label className="inline-label">
          Before <input type="datetime-local" value={form.created_before} onChange={set('created_before')} />
        </label>
        <input type="number" min={1} placeholder="Max attempts" value={form.max_attempts} onChange={set('max_attempts')} aria-label="Max attempts" />
        <button type="button" className="btn btn-small btn-ghost" onClick={() => setForm(EMPTY)}>
          Clear
        </button>
      </form>

      <div className="replay-bar panel">
        <div>
          {preview === null ? (
            <span className="muted">Preview how many entries the current filter would replay.</span>
          ) : (
            <span>
              <strong>{formatNumber(preview)}</strong> entries match this filter.
            </span>
          )}
        </div>
        <div className="row-actions">
          <button type="button" className="btn" onClick={runPreview} disabled={previewing}>
            {previewing ? 'Counting…' : 'Dry run'}
          </button>
          <button type="button" className="btn" disabled={selected.size === 0 || Boolean(job && !job.done)} onClick={() => setConfirm('selected')}>
            Replay selected ({selected.size})
          </button>
          <button type="button" className="btn btn-primary" disabled={preview === null || preview === 0 || Boolean(job && !job.done)} onClick={() => setConfirm('filter')}>
            Replay all matching
          </button>
        </div>
        {job && (
          <div className="progress-wrap" aria-live="polite">
            <progress max={Math.max(1, job.total)} value={job.processed} />
            <span className="small">
              {formatNumber(job.processed)} / {formatNumber(job.total)} processed · {formatNumber(job.succeeded)} delivered ·{' '}
              {formatNumber(job.failed)} failed {job.done ? '· done' : ''}
            </span>
          </div>
        )}
      </div>

      <section className="panel">
        <AsyncBoundary
          loading={entries.loading && !entries.data}
          error={entries.error}
          onRetry={entries.reload}
          empty={list.length === 0}
          emptyMessage="The dead-letter queue is empty for this filter."
        >
          <div className="table-wrap">
            <table className="data-table">
              <thead>
                <tr>
                  <th>
                    <input
                      type="checkbox"
                      aria-label="Select all on this page"
                      checked={allOnPage}
                      onChange={(e) =>
                        setSelected((s) => {
                          const next = new Set(s);
                          for (const x of list) {
                            if (e.target.checked) next.add(x.id);
                            else next.delete(x.id);
                          }
                          return next;
                        })
                      }
                    />
                  </th>
                  <th>Endpoint</th>
                  <th>HTTP</th>
                  <th>Reason</th>
                  <th className="num">Attempts</th>
                  <th>Failed</th>
                </tr>
              </thead>
              <tbody>
                {list.map((e) => (
                  <tr key={e.id}>
                    <td>
                      <input
                        type="checkbox"
                        aria-label={`Select ${e.id}`}
                        checked={selected.has(e.id)}
                        onChange={(ev) =>
                          setSelected((s) => {
                            const next = new Set(s);
                            if (ev.target.checked) next.add(e.id);
                            else next.delete(e.id);
                            return next;
                          })
                        }
                      />
                    </td>
                    <td className="mono small">{maskUrl(e.url)}</td>
                    <td>
                      <HttpStatus code={e.status_code} />
                    </td>
                    <td className="truncate small" title={e.failure_reason ?? undefined}>
                      {e.failure_reason ?? '—'}
                    </td>
                    <td className="num">{e.attempts}</td>
                    <td>{formatRelative(e.last_attempt_at ?? e.created_at)}</td>
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
      </section>

      <ConfirmDialog
        open={confirm !== null}
        title="Replay dead-lettered deliveries"
        message={
          confirm === 'filter'
            ? `Re-queue ${formatNumber(preview ?? 0)} entries matching the current filter? Receivers may see duplicates if they already processed an earlier attempt.`
            : `Re-queue ${selected.size} selected entries?`
        }
        confirmLabel="Replay"
        onClose={() => setConfirm(null)}
        onConfirm={() => startReplay(confirm ?? 'selected')}
      />
    </div>
  );
}
