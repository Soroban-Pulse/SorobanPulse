import { useEffect, useMemo, useState } from "react";
import { useOutletContext } from "react-router-dom";
import { ConfirmDialog } from "../components/Dialog";
import type { LayoutContext } from "../components/Layout";
import { api, ApiError, errorMessage, qs } from "../lib/api";
import { lagLevel } from "../lib/health";
import { formatDateTime, formatDuration, formatNumber } from "../lib/format";
import { useSettings } from "../lib/settings";
import { readJson, writeJson } from "../lib/storage";
import type { AuditLogEntry, EventPage, IndexerStatus, LedgerGap, PublicConfig } from "../lib/types";
import { usePolling } from "../hooks/usePolling";

// Issue #1103: operator console for the indexer.

/** Poll fast enough that a pause shows up in the UI well within 5 seconds. */
const STATUS_POLL_MS = 2_000;
const AUDIT_POLL_MS = 15_000;
const MAX_REPLAY_RANGE = 10_000; // mirrors the server-side limit in replay_events
const DEFAULT_LAG_THRESHOLD = 100;

type Confirm = { title: string; message: React.ReactNode; confirmLabel: string; danger?: boolean; run: () => Promise<void> };

export function AdminPage() {
  const settings = useSettings();
  const { openSettings } = useOutletContext<LayoutContext>();
  const [confirm, setConfirm] = useState<Confirm | null>(null);
  const [busy, setBusy] = useState(false);
  const [auditTick, setAuditTick] = useState(0);

  const status = usePolling(() => api<IndexerStatus>("/v1/status", { auth: "admin" }), STATUS_POLL_MS, [
    settings.adminKey,
    settings.apiBaseUrl,
  ]);
  const config = usePolling(() => api<PublicConfig>("/v1/config", { auth: "admin" }), 0, [settings.apiBaseUrl]);
  const threshold = config.data?.indexer_lag_warn_threshold || DEFAULT_LAG_THRESHOLD;

  if (!settings.adminKey) {
    return (
      <div className="card empty">
        <h1>Admin console</h1>
        <p>An admin key is required to control the indexer.</p>
        <button type="button" className="primary" onClick={openSettings}>
          Add admin key
        </button>
      </div>
    );
  }

  const runConfirm = async () => {
    if (!confirm) return;
    setBusy(true);
    try {
      await confirm.run();
    } finally {
      setBusy(false);
      setConfirm(null);
      setAuditTick((t) => t + 1);
    }
  };

  return (
    <div>
      <div className="page-head">
        <h1>Admin console</h1>
      </div>
      <IndexerPanel
        status={status.data}
        error={status.error}
        threshold={threshold}
        refresh={status.refresh}
        requestConfirm={setConfirm}
      />
      <div className="grid-2" style={{ marginTop: 16 }}>
        <ReplayPanel status={status.data} requestConfirm={setConfirm} />
        <GapsPanel requestConfirm={setConfirm} />
      </div>
      <AuditPanel tick={auditTick} />
      <ConfirmDialog
        open={!!confirm}
        title={confirm?.title ?? ""}
        message={confirm?.message}
        confirmLabel={confirm?.confirmLabel ?? "Confirm"}
        danger={confirm?.danger}
        busy={busy}
        onConfirm={runConfirm}
        onCancel={() => !busy && setConfirm(null)}
      />
    </div>
  );
}

// ── Indexer status + pause/resume ──────────────────────────────────────────

function IndexerPanel({
  status,
  error,
  threshold,
  refresh,
  requestConfirm,
}: {
  status: IndexerStatus | undefined;
  error: unknown;
  threshold: number;
  refresh: () => Promise<void>;
  requestConfirm: (c: Confirm) => void;
}) {
  // Desired paused state after a click; cleared once /v1/status agrees.
  const [pending, setPending] = useState<boolean | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  useEffect(() => {
    if (status && pending !== null && status.indexer_paused === pending) setPending(null);
  }, [status, pending]);

  const paused = status?.indexer_paused ?? false;
  const isLeader = status?.indexer_mode === "active";
  const lvl = status ? lagLevel(status.lag_ledgers, threshold) : "neutral";

  const toggle = () => {
    const target = !paused;
    requestConfirm({
      title: target ? "Pause indexer?" : "Resume indexer?",
      message: target ? (
        <p>
          The indexer will stop ingesting new ledgers until resumed. The API keeps serving already-indexed data, and lag
          will grow while paused.
        </p>
      ) : (
        <p>The indexer will resume from ledger {formatNumber(status?.current_ledger)} and catch up to the network tip.</p>
      ),
      confirmLabel: target ? "Pause indexer" : "Resume indexer",
      danger: target,
      run: async () => {
        setActionError(null);
        try {
          await api(`/v1/admin/indexer/${target ? "pause" : "resume"}`, { method: "POST", auth: "admin" });
          setPending(target);
          await refresh();
        } catch (err) {
          setActionError(errorMessage(err));
        }
      },
    });
  };

  return (
    <div className="card">
      <div className="card-header">
        <h2>Indexer</h2>
        <div className="row" style={{ alignItems: "center" }}>
          {status && (
            <span className={`badge ${status.indexer_status === "stalled" ? "danger" : paused ? "warn" : "ok"}`}>
              {status.indexer_status === "stalled" ? "Stalled" : paused ? "Paused" : "Running"}
            </span>
          )}
          <button
            type="button"
            className={paused ? "primary" : "danger"}
            onClick={toggle}
            disabled={!status || !isLeader || pending !== null}
            title={status && !isLeader ? "This replica is not the leader; pause/resume must target the active indexer" : undefined}
          >
            {pending === true ? "Pausing…" : pending === false ? "Resuming…" : paused ? "▶ Resume" : "⏸ Pause"}
          </button>
        </div>
      </div>
      {error && !status ? <div className="notice danger">{errorMessage(error)}</div> : null}
      {actionError && <div className="notice danger" style={{ marginBottom: 12 }}>{actionError}</div>}
      {status && (
        <div className="stats">
          <Stat label="Current ledger" value={formatNumber(status.current_ledger)} />
          <Stat label="Network tip" value={formatNumber(status.latest_ledger)} />
          <Stat
            label={`Lag (warn > ${formatNumber(threshold)})`}
            value={<span className={`${lvl}-text`}>{formatNumber(status.lag_ledgers)}</span>}
          />
          <Stat
            label="Replica role"
            value={
              <span className={`badge ${isLeader ? "ok" : "neutral"}`} style={{ fontSize: 13 }}>
                {isLeader ? "Leader (active)" : "Follower (read-only)"}
              </span>
            }
          />
          <Stat label="Total events" value={formatNumber(status.total_events)} />
          <Stat label="Uptime" value={formatDuration(status.uptime_secs)} />
          <Stat label="Version" value={status.version} />
        </div>
      )}
      {status && !isLeader && (
        <p className="muted" style={{ marginBottom: 0 }}>
          This API replica does not hold the indexer lock. Point the API base URL at the leader to pause, resume or replay.
        </p>
      )}
    </div>
  );
}

function Stat({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="stat">
      <div className="label">{label}</div>
      <div className="value">{value}</div>
    </div>
  );
}

// ── Replay ─────────────────────────────────────────────────────────────────

interface ReplayJob {
  id: string;
  from: number;
  to: number;
  submittedAt: number;
  state: "accepted" | "failed" | "running" | "completed";
  message?: string;
  warning?: string;
  /** Server-provided job id / status URL, when the server exposes job tracking. */
  jobId?: string;
  statusUrl?: string;
  progress?: number;
}

const JOBS_KEY = "sp.replayJobs";

function useReplayJobs() {
  const [jobs, setJobs] = useState<ReplayJob[]>(() => readJson<ReplayJob[]>(JOBS_KEY, []));
  useEffect(() => writeJson(JOBS_KEY, jobs.slice(0, 20)), [jobs]);
  const update = (id: string, patch: Partial<ReplayJob>) =>
    setJobs((js) => js.map((j) => (j.id === id ? { ...j, ...patch } : j)));
  return { jobs, setJobs, update };
}

async function submitReplay(from: number, to: number): Promise<Omit<ReplayJob, "id" | "submittedAt">> {
  try {
    const res = await api<Record<string, unknown>>("/v1/admin/replay", {
      method: "POST",
      auth: "admin",
      body: { from_ledger: from, to_ledger: to },
    });
    const jobId = (res.job_id ?? res.id) as string | undefined;
    const statusUrl = (res.status_url as string | undefined) ?? (jobId ? `/v1/admin/replay/${jobId}` : undefined);
    return {
      from,
      to,
      state: "accepted",
      message: (res.message as string) ?? "replay job accepted",
      warning: res.warning as string | undefined,
      jobId,
      statusUrl,
    };
  } catch (err) {
    return { from, to, state: "failed", message: errorMessage(err) };
  }
}

/** Indexed window, derived from the oldest and newest indexed events. */
function useIndexedRange() {
  return usePolling(async () => {
    const edge = (sort: "asc" | "desc") =>
      api<EventPage>(`/v1/events${qs({ limit: 1, sort, sort_by: "ledger" })}`, { auth: "admin" }).then(
        (p) => p.data?.[0]?.ledger ?? null,
      );
    const [min, max] = await Promise.all([edge("asc"), edge("desc")]);
    return { min, max };
  }, 60_000);
}

function ReplayPanel({ status, requestConfirm }: { status: IndexerStatus | undefined; requestConfirm: (c: Confirm) => void }) {
  const range = useIndexedRange();
  const { jobs, setJobs, update } = useReplayJobs();
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [touched, setTouched] = useState(false);

  const min = range.data?.min ?? null;
  const max = range.data?.max ?? null;

  const validation = useMemo(() => {
    const f = Number(from);
    const t = Number(to);
    if (!from || !to) return { error: "Enter a start and end ledger." };
    if (!Number.isInteger(f) || !Number.isInteger(t) || f <= 0 || t <= 0) return { error: "Ledgers must be positive integers." };
    if (f > t) return { error: "Start ledger must be ≤ end ledger." };
    if (t - f + 1 > MAX_REPLAY_RANGE) return { error: `Range cannot exceed ${formatNumber(MAX_REPLAY_RANGE)} ledgers.` };
    if (min === null || max === null) return { error: "No events have been indexed yet, so there is nothing to replay." };
    if (t < min || f > max) return { error: `Range is entirely outside the indexed window ${formatNumber(min)}–${formatNumber(max)}.` };
    if (f < min || t > max) {
      return {
        warning: `Only ledgers ${formatNumber(Math.max(f, min))}–${formatNumber(Math.min(t, max))} are indexed and will be replayed.`,
      };
    }
    return {};
  }, [from, to, min, max]);

  // Poll server-side job status for jobs that expose it.
  useEffect(() => {
    const active = jobs.filter((j) => j.statusUrl && (j.state === "accepted" || j.state === "running"));
    if (!active.length) return;
    const timer = setInterval(() => {
      active.forEach(async (j) => {
        try {
          const s = await api<Record<string, unknown>>(j.statusUrl!, { auth: "admin" });
          const state = String(s.status ?? s.state ?? "running").toLowerCase();
          const progress =
            typeof s.progress === "number"
              ? s.progress
              : typeof s.processed_ledgers === "number" && j.to >= j.from
                ? (s.processed_ledgers as number) / (j.to - j.from + 1)
                : undefined;
          update(j.id, {
            state: state.includes("complete") || state === "done" ? "completed" : state.includes("fail") ? "failed" : "running",
            progress,
            message: (s.error as string) ?? (s.message as string) ?? undefined,
          });
        } catch (err) {
          // The server does not expose replay job status — stop polling this job.
          if (err instanceof ApiError && err.status === 404) update(j.id, { statusUrl: undefined });
        }
      });
    }, 3_000);
    return () => clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [jobs]);

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    setTouched(true);
    if (validation.error) return;
    const f = Number(from);
    const t = Number(to);
    requestConfirm({
      title: "Start replay?",
      message: (
        <>
          <p>
            Re-fetch and re-index ledgers <strong>{formatNumber(f)}–{formatNumber(t)}</strong> ({formatNumber(t - f + 1)} ledgers) from the
            RPC endpoint.
          </p>
          {validation.warning && <div className="notice warn">{validation.warning}</div>}
        </>
      ),
      confirmLabel: "Start replay",
      run: async () => {
        const result = await submitReplay(f, t);
        setJobs((js) => [{ ...result, id: `${Date.now()}-${Math.random().toString(36).slice(2)}`, submittedAt: Date.now() }, ...js]);
        if (result.state !== "failed") {
          setFrom("");
          setTo("");
          setTouched(false);
        }
      },
    });
  };

  const isLeader = status?.indexer_mode === "active";

  return (
    <div className="card">
      <div className="card-header">
        <h2>Replay ledger range</h2>
      </div>
      <p className="muted" style={{ marginTop: 0 }}>
        Indexed window:{" "}
        {range.loading ? "loading…" : min === null ? "nothing indexed yet" : `${formatNumber(min)} – ${formatNumber(max)}`}
      </p>
      <form onSubmit={submit} noValidate>
        <div className="row">
          <label className="field">
            From ledger
            <input
              inputMode="numeric"
              value={from}
              onChange={(e) => setFrom(e.target.value.replace(/[^\d]/g, ""))}
              aria-invalid={touched && !!validation.error}
              style={{ width: 140 }}
            />
          </label>
          <label className="field">
            To ledger
            <input
              inputMode="numeric"
              value={to}
              onChange={(e) => setTo(e.target.value.replace(/[^\d]/g, ""))}
              aria-invalid={touched && !!validation.error}
              style={{ width: 140 }}
            />
          </label>
          <button type="submit" className="primary" disabled={!isLeader}>
            Replay
          </button>
        </div>
        {(touched || (from && to)) && validation.error && (
          <p className="danger-text" role="alert">
            {validation.error}
          </p>
        )}
        {validation.warning && <p className="warn-text">{validation.warning}</p>}
      </form>

      {jobs.length > 0 && (
        <>
          <div className="card-header" style={{ marginTop: 16 }}>
            <h3 style={{ margin: 0 }}>Recent jobs</h3>
            <button type="button" className="ghost" onClick={() => setJobs([])}>
              Clear
            </button>
          </div>
          <ul style={{ listStyle: "none", padding: 0, margin: 0 }}>
            {jobs.map((j) => (
              <li key={j.id} style={{ padding: "8px 0", borderTop: "1px solid var(--color-border)" }}>
                <div className="row" style={{ justifyContent: "space-between", alignItems: "center" }}>
                  <span className="mono">
                    {formatNumber(j.from)}–{formatNumber(j.to)}
                  </span>
                  <span className={`badge ${j.state === "failed" ? "danger" : j.state === "completed" ? "ok" : "accent"}`}>
                    {j.state}
                  </span>
                </div>
                {j.progress !== undefined && (
                  <div className="progress" style={{ marginTop: 6 }} aria-label="Replay progress">
                    <span style={{ width: `${Math.min(100, Math.round(j.progress * 100))}%` }} />
                  </div>
                )}
                <div className="muted" style={{ fontSize: 12 }}>
                  {new Date(j.submittedAt).toLocaleString()}
                  {j.message ? ` · ${j.message}` : ""}
                  {j.state === "accepted" && !j.statusUrl ? " · running in the background (server does not report progress)" : ""}
                </div>
                {j.warning && <div className="warn-text" style={{ fontSize: 12 }}>{j.warning}</div>}
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

// ── Gaps + backfill ────────────────────────────────────────────────────────

function chunks(from: number, to: number): [number, number][] {
  const out: [number, number][] = [];
  for (let start = from; start <= to; start += MAX_REPLAY_RANGE) {
    out.push([start, Math.min(to, start + MAX_REPLAY_RANGE - 1)]);
  }
  return out;
}

function GapsPanel({ requestConfirm }: { requestConfirm: (c: Confirm) => void }) {
  const gaps = usePolling(async () => {
    try {
      const res = await api<{ data?: LedgerGap[]; gaps?: LedgerGap[] }>("/v1/admin/indexer/gaps", { auth: "admin" });
      return { available: true, gaps: res.data ?? res.gaps ?? [] };
    } catch (err) {
      if (err instanceof ApiError && (err.status === 404 || err.status === 405)) return { available: false, gaps: [] };
      throw err;
    }
  }, 60_000);
  const [results, setResults] = useState<Record<string, string>>({});

  const backfill = (gap: LedgerGap) => {
    const parts = chunks(gap.from_ledger, gap.to_ledger);
    const key = `${gap.from_ledger}-${gap.to_ledger}`;
    requestConfirm({
      title: "Backfill gap?",
      message: (
        <p>
          Replay ledgers <strong>{formatNumber(gap.from_ledger)}–{formatNumber(gap.to_ledger)}</strong>
          {parts.length > 1 ? ` as ${parts.length} replay jobs of up to ${formatNumber(MAX_REPLAY_RANGE)} ledgers` : ""}. Ledgers
          older than the RPC retention window cannot be recovered this way.
        </p>
      ),
      confirmLabel: "Backfill",
      run: async () => {
        const out = [];
        for (const [f, t] of parts) out.push(await submitReplay(f, t));
        const failed = out.filter((r) => r.state === "failed");
        setResults((r) => ({
          ...r,
          [key]: failed.length ? `Failed: ${failed[0].message}` : `${out.length} replay job(s) accepted`,
        }));
        gaps.refresh();
      },
    });
  };

  return (
    <div className="card">
      <div className="card-header">
        <h2>Ledger gaps</h2>
        <button type="button" className="ghost" onClick={() => gaps.refresh()}>
          Refresh
        </button>
      </div>
      {gaps.error ? (
        <div className="notice danger">{errorMessage(gaps.error)}</div>
      ) : gaps.loading && !gaps.data ? (
        <div className="muted">Loading…</div>
      ) : !gaps.data?.available ? (
        <div className="notice">
          This server does not expose gap detection (<code>GET /v1/admin/indexer/gaps</code>). Use the replay form to backfill a
          known range.
        </div>
      ) : gaps.data.gaps.length === 0 ? (
        <div className="empty">No gaps detected in the indexed range.</div>
      ) : (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>From</th>
                <th>To</th>
                <th>Ledgers</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {gaps.data.gaps.map((g) => {
                const key = `${g.from_ledger}-${g.to_ledger}`;
                return (
                  <tr key={key}>
                    <td className="mono">{formatNumber(g.from_ledger)}</td>
                    <td className="mono">{formatNumber(g.to_ledger)}</td>
                    <td>
                      {formatNumber(g.to_ledger - g.from_ledger + 1)}
                      {g.reason && <div className="muted" style={{ fontSize: 12 }}>{g.reason}</div>}
                      {results[key] && <div className="muted" style={{ fontSize: 12 }}>{results[key]}</div>}
                    </td>
                    <td>
                      <button type="button" onClick={() => backfill(g)}>
                        Backfill
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

// ── Audit log ──────────────────────────────────────────────────────────────

function AuditPanel({ tick }: { tick: number }) {
  const logs = usePolling(
    () => api<{ data: AuditLogEntry[]; total: number }>(`/v1/admin/audit-logs${qs({ limit: 25 })}`, { auth: "admin" }),
    AUDIT_POLL_MS,
    [tick],
  );
  const entries = logs.data?.data ?? [];

  return (
    <div className="card" style={{ marginTop: 16 }}>
      <div className="card-header">
        <h2>Recent admin actions</h2>
        {logs.data && <span className="muted">{formatNumber(logs.data.total)} total</span>}
      </div>
      {logs.error ? (
        <div className="notice danger">Audit log unavailable: {errorMessage(logs.error)}</div>
      ) : !entries.length ? (
        <div className="empty">{logs.loading ? "Loading…" : "No audit entries yet."}</div>
      ) : (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Time</th>
                <th>Action</th>
                <th>Resource</th>
                <th>Actor</th>
                <th>Result</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((e) => (
                <tr key={e.id}>
                  <td className="muted" style={{ whiteSpace: "nowrap" }}>
                    {formatDateTime(e.created_at)}
                  </td>
                  <td>
                    <div>{e.action ?? e.event_type}</div>
                    {e.request_path && (
                      <div className="muted mono" style={{ fontSize: 11 }}>
                        {e.request_method} {e.request_path}
                      </div>
                    )}
                  </td>
                  <td>
                    {e.resource_type}
                    {e.resource_id && <div className="muted mono truncate" style={{ fontSize: 11, maxWidth: 220 }}>{e.resource_id}</div>}
                  </td>
                  <td className="muted">{e.user_email ?? e.created_by ?? "—"}</td>
                  <td>
                    <span className={`badge ${e.success === false ? "danger" : e.severity === "warning" ? "warn" : "ok"}`}>
                      {e.success === false ? `failed${e.status_code ? ` (${e.status_code})` : ""}` : "ok"}
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
