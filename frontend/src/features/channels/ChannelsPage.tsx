import { useMemo, useState } from 'react';
import { errorMessage } from '../../api/client';
import { channelsApi } from '../../api/channels';
import type { HealthState, NotificationChannel } from '../../api/types';
import { AsyncBoundary } from '../../components/AsyncBoundary';
import { ConfirmDialog } from '../../components/ConfirmDialog';
import { StatusBadge } from '../../components/StatusBadge';
import { useToast } from '../../components/Toast';
import { formatRelative } from '../../lib/format';
import { useAsync, useDebounced } from '../../lib/useAsync';
import { ChannelEditor, type EditorMode } from './ChannelEditor';
import { ChannelGroupsPanel } from './ChannelGroupsPanel';
import { ChannelIcon } from './ChannelIcon';
import { ChannelSparkline } from './ChannelSparkline';
import { MaintenanceWindowsPanel } from './MaintenanceWindowsPanel';
import { CHANNEL_TYPES, SCHEMAS } from './schema';

interface TestState {
  running: boolean;
  success?: boolean;
  message?: string;
  at?: string;
}

export function ChannelsPage() {
  const toast = useToast();
  const [q, setQ] = useState('');
  const [type, setType] = useState('');
  const [status, setStatus] = useState('');
  const [tag, setTag] = useState('');
  const [editor, setEditor] = useState<EditorMode | null>(null);
  const [deleting, setDeleting] = useState<NotificationChannel | null>(null);
  const [tests, setTests] = useState<Record<string, TestState>>({});

  const debouncedQ = useDebounced(q);
  const channels = useAsync(
    () => channelsApi.list({ q: debouncedQ || undefined, channel_type: type || undefined, status: status || undefined, tag: tag || undefined, page_size: 100 }),
    [debouncedQ, type, status, tag],
  );

  // Health comes from the background checker; fall back to 30-day success rate
  // from the analytics dashboard when the health endpoint is unavailable.
  const health = useAsync(async () => {
    const map = new Map<string, { state: HealthState; detail?: string }>();
    try {
      const res = await channelsApi.health();
      for (const h of res.data ?? []) {
        map.set(h.channel_id, {
          state: h.healthy === null ? 'unknown' : h.healthy ? 'healthy' : 'unhealthy',
          detail: h.error ?? (h.checked_at ? `checked ${formatRelative(h.checked_at)}` : undefined),
        });
      }
    } catch {
      const dash = await channelsApi.dashboard();
      for (const c of dash.channels) {
        map.set(c.channel_id, {
          state: c.sent_30d === 0 ? 'unknown' : c.success_rate >= 0.95 ? 'healthy' : 'unhealthy',
          detail: `${(c.success_rate * 100).toFixed(1)}% delivered (30d)`,
        });
      }
    }
    return map;
  }, []);

  const list = channels.data?.data ?? [];
  const allTags = useMemo(() => Array.from(new Set(list.flatMap((c) => c.tags))).sort(), [list]);

  const runTest = async (c: NotificationChannel) => {
    setTests((t) => ({ ...t, [c.id]: { running: true } }));
    try {
      const res = await channelsApi.test(c.id);
      setTests((t) => ({ ...t, [c.id]: { running: false, success: res.success, message: res.subject, at: new Date().toISOString() } }));
      toast(res.success ? `Test delivered to ${c.name}` : `Test to ${c.name} failed`, res.success ? 'ok' : 'bad');
    } catch (e) {
      // The test endpoint answers 502 with a JSON body when delivery fails.
      const msg = errorMessage(e);
      setTests((t) => ({ ...t, [c.id]: { running: false, success: false, message: msg, at: new Date().toISOString() } }));
      toast(`Test to ${c.name} failed: ${msg}`, 'bad');
    }
  };

  const toggle = async (c: NotificationChannel) => {
    const enable = c.status === 'disabled';
    // Optimistic: flip locally, roll back on failure.
    channels.setData((d) => d && { ...d, data: d.data.map((x) => (x.id === c.id ? { ...x, status: enable ? 'active' : 'disabled' } : x)) });
    try {
      await channelsApi.setEnabled(c.id, enable);
      toast(`${c.name} ${enable ? 'enabled' : 'disabled'}`, 'ok');
    } catch (e) {
      channels.setData((d) => d && { ...d, data: d.data.map((x) => (x.id === c.id ? c : x)) });
      toast(errorMessage(e), 'bad');
    }
  };

  return (
    <div className="page">
      <header className="page-header">
        <div>
          <h1>Notification channels</h1>
          <p className="muted">Slack, Discord, Telegram, email, SMS, PagerDuty and GitHub destinations for alerts.</p>
        </div>
        <button type="button" className="btn btn-primary" onClick={() => setEditor({ kind: 'create' })}>
          New channel
        </button>
      </header>

      <div className="filters" role="search">
        <input type="search" placeholder="Search name or description" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Search channels" />
        <select value={type} onChange={(e) => setType(e.target.value)} aria-label="Channel type">
          <option value="">All types</option>
          {CHANNEL_TYPES.map((t) => (
            <option key={t} value={t}>
              {SCHEMAS[t].label}
            </option>
          ))}
        </select>
        <select value={status} onChange={(e) => setStatus(e.target.value)} aria-label="Status">
          <option value="">Any status</option>
          <option value="active">Enabled</option>
          <option value="disabled">Disabled</option>
        </select>
        <select value={tag} onChange={(e) => setTag(e.target.value)} aria-label="Tag">
          <option value="">Any tag</option>
          {allTags.map((t) => (
            <option key={t} value={t}>
              {t}
            </option>
          ))}
        </select>
      </div>

      <section className="panel">
        <AsyncBoundary
          loading={channels.loading && !channels.data}
          error={channels.error}
          onRetry={channels.reload}
          empty={list.length === 0}
          emptyMessage="No channels match. Create one to start routing notifications."
        >
          <div className="table-wrap">
            <table className="data-table">
              <thead>
                <tr>
                  <th>Channel</th>
                  <th>Health</th>
                  <th>Status</th>
                  <th>Tags</th>
                  <th>Deliveries (7d)</th>
                  <th>Last test</th>
                  <th className="actions-col">Actions</th>
                </tr>
              </thead>
              <tbody>
                {list.map((c) => {
                  const h = health.data?.get(c.id) ?? { state: 'unknown' as HealthState };
                  const t = tests[c.id];
                  return (
                    <tr key={c.id} className={c.status === 'disabled' ? 'row-muted' : undefined}>
                      <td>
                        <div className="channel-cell">
                          <ChannelIcon type={c.channel_type} />
                          <div>
                            <strong>{c.name}</strong>
                            <div className="muted small">
                              {SCHEMAS[c.channel_type]?.label ?? c.channel_type}
                              {c.description ? ` · ${c.description}` : ''}
                            </div>
                          </div>
                        </div>
                      </td>
                      <td title={h.detail}>
                        <StatusBadge status={h.state} />
                      </td>
                      <td>
                        <StatusBadge status={c.status === 'disabled' ? 'disabled' : 'active'} label={c.status === 'disabled' ? 'disabled' : 'enabled'} />
                      </td>
                      <td>
                        <ul className="chip-list">
                          {c.tags.map((tg) => (
                            <li key={tg}>
                              <button type="button" className="chip chip-btn" onClick={() => setTag(tg)} title={`Filter by ${tg}`}>
                                {tg}
                              </button>
                            </li>
                          ))}
                        </ul>
                      </td>
                      <td>
                        <ChannelSparkline channelId={c.id} name={c.name} />
                      </td>
                      <td>
                        {t?.running ? (
                          <span className="muted">sending…</span>
                        ) : t ? (
                          <span title={t.message}>
                            <StatusBadge status={t.success ? 'success' : 'failed'} label={t.success ? 'delivered' : 'failed'} />{' '}
                            <span className="muted small">{formatRelative(t.at)}</span>
                          </span>
                        ) : (
                          <span className="muted">—</span>
                        )}
                      </td>
                      <td className="actions-col">
                        <div className="row-actions">
                          <button type="button" className="btn btn-small" onClick={() => runTest(c)} disabled={t?.running}>
                            Test
                          </button>
                          <button type="button" className="btn btn-small" onClick={() => toggle(c)}>
                            {c.status === 'disabled' ? 'Enable' : 'Disable'}
                          </button>
                          <button type="button" className="btn btn-small btn-ghost" onClick={() => setEditor({ kind: 'edit', channel: c })}>
                            Edit
                          </button>
                          <button type="button" className="btn btn-small btn-ghost" onClick={() => setEditor({ kind: 'clone', channel: c })}>
                            Clone
                          </button>
                          <button type="button" className="btn btn-small btn-ghost danger-text" onClick={() => setDeleting(c)}>
                            Delete
                          </button>
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </AsyncBoundary>
      </section>

      <div className="two-col">
        <ChannelGroupsPanel channels={list} />
        <MaintenanceWindowsPanel />
      </div>

      <ChannelEditor
        mode={editor}
        onClose={() => setEditor(null)}
        onSaved={(saved, { test }) => {
          const wasEdit = editor?.kind === 'edit';
          setEditor(null);
          toast(`${saved.name} ${wasEdit ? 'updated' : 'created'}`, 'ok');
          channels.reload();
          if (test) runTest(saved);
        }}
      />

      <ConfirmDialog
        open={deleting !== null}
        title={`Delete ${deleting?.name ?? 'channel'}`}
        message="Notifications routed to this channel will stop. Groups referencing it lose this member."
        confirmLabel="Delete channel"
        danger
        typeToConfirm={deleting?.name}
        onClose={() => setDeleting(null)}
        onConfirm={async () => {
          if (!deleting) return;
          await channelsApi.remove(deleting.id);
          toast(`${deleting.name} deleted`, 'ok');
          channels.reload();
        }}
      />
    </div>
  );
}
