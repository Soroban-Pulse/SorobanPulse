import { useSearchParams } from "react-router-dom";
import { api, errorMessage, requestRaw, type RawResponse } from "../lib/api";
import { lagLevel, type Level } from "../lib/health";
import { formatDuration, formatNumber, formatPercent } from "../lib/format";
import { useSettings } from "../lib/settings";
import { readJson, writeJson } from "../lib/storage";
import type { ComponentHealth, IndexerStatus, OverallHealth, PublicConfig, SloAggregateReport } from "../lib/types";
import { useNow, usePolling } from "../hooks/usePolling";

// Issue #1104: public-friendly status page over /health, /healthz/*, /v1/status
// and the SLO report.

const REFRESH_MS = 15_000;
const EXTERNAL_SERVICES = ["webhooks", "email", "sms", "pagerduty"] as const;
const DEFAULT_LAG_THRESHOLD = 100;

interface Tile {
  id: string;
  name: string;
  level: Level;
  summary: string;
  detail?: string;
}

const LEVEL_LABEL: Record<Level, string> = { ok: "Operational", warn: "Degraded", danger: "Down", neutral: "Unknown" };
const RANK: Record<Level, number> = { neutral: 0, ok: 1, warn: 2, danger: 3 };

function worst(levels: Level[]): Level {
  return levels.reduce<Level>((acc, l) => (RANK[l] > RANK[acc] ? l : acc), "ok");
}

function fromHealth(status: string | undefined): Level {
  if (status === "ok") return "ok";
  if (status === "degraded") return "warn";
  if (status === "unhealthy") return "danger";
  return "neutral";
}

async function timed<T>(path: string, auth: "none" | "user" = "none") {
  const start = performance.now();
  try {
    const res = await requestRaw<T>(path, { auth, timeoutMs: 8_000 });
    return { res, ms: Math.round(performance.now() - start), error: null as string | null };
  } catch (err) {
    return { res: null as RawResponse<T> | null, ms: 0, error: errorMessage(err) };
  }
}

function componentTile(
  id: string,
  name: string,
  r: Awaited<ReturnType<typeof timed<ComponentHealth>>>,
  showDetail: boolean,
  extra?: (h: ComponentHealth) => string | undefined,
): Tile {
  if (!r.res) return { id, name, level: "danger", summary: "Unreachable", detail: showDetail ? r.error ?? undefined : undefined };
  const h = r.res.data;
  if (!h?.status) {
    return { id, name, level: r.res.ok ? "neutral" : "danger", summary: `HTTP ${r.res.status}` };
  }
  const level = fromHealth(h.status);
  const parts = [`${h.response_time_ms ?? r.ms} ms`];
  if (showDetail) {
    const e = extra?.(h);
    if (e) parts.push(e);
    if (h.message && level !== "ok") parts.push(h.message);
  }
  return { id, name, level, summary: LEVEL_LABEL[level], detail: parts.join(" · ") };
}

async function loadTiles(publicMode: boolean): Promise<Tile[]> {
  const [health, pg, rpc, status, config, ...external] = await Promise.all([
    timed<OverallHealth>("/health"),
    timed<ComponentHealth>("/healthz/postgres"),
    timed<ComponentHealth>("/healthz/rpc"),
    publicMode ? null : timed<IndexerStatus>("/v1/status", "user"),
    publicMode ? null : timed<PublicConfig>("/v1/config", "user"),
    ...(publicMode ? [] : EXTERNAL_SERVICES.map((s) => timed<ComponentHealth>(`/healthz/external/${s}`))),
  ]);

  const tiles: Tile[] = [];

  // API: the process answered at all (a 503 from /health still means the API is up).
  tiles.push(
    health.res
      ? { id: "api", name: "API", level: "ok", summary: "Operational", detail: `${health.ms} ms` }
      : { id: "api", name: "API", level: "danger", summary: "Unreachable", detail: publicMode ? undefined : health.error ?? undefined },
  );

  tiles.push(
    componentTile("db", "Database", pg, !publicMode, (h) =>
      h.pool_size != null ? `pool ${h.idle_connections ?? 0}/${h.pool_size} idle` : undefined,
    ),
  );
  tiles.push(componentTile("rpc", "RPC endpoint", rpc, !publicMode, (h) => h.active_endpoint ?? undefined));

  // Indexer: prefer the detailed /v1/status, fall back to /health.
  const st = status?.res?.ok ? status.res.data : undefined;
  if (st) {
    const threshold = (config?.res?.ok && config.res.data?.indexer_lag_warn_threshold) || DEFAULT_LAG_THRESHOLD;
    let level = lagLevel(st.lag_ledgers, threshold);
    let summary = level === "ok" ? "Operational" : "Lagging";
    if (st.indexer_paused) {
      level = "warn";
      summary = "Paused";
    }
    if (st.indexer_status === "stalled") {
      level = "danger";
      summary = "Stalled";
    }
    tiles.push({
      id: "indexer",
      name: "Indexer",
      level,
      summary,
      detail: `ledger ${formatNumber(st.current_ledger)} · lag ${formatNumber(st.lag_ledgers)} (warn > ${threshold})`,
    });
  } else {
    const h = health.res?.data;
    const stalled = h?.indexer === "stalled";
    tiles.push({
      id: "indexer",
      name: "Indexer",
      level: !h ? "neutral" : stalled ? "danger" : "ok",
      summary: !h ? "Unknown" : stalled ? "Stalled" : "Operational",
      detail:
        stalled && h?.last_poll_secs_ago != null && !publicMode
          ? `last poll ${formatDuration(h.last_poll_secs_ago)} ago`
          : undefined,
    });
  }

  EXTERNAL_SERVICES.forEach((svc, i) => {
    const r = external[i];
    if (r) tiles.push(componentTile(`ext:${svc}`, svc[0].toUpperCase() + svc.slice(1), r, true));
  });

  return tiles;
}

// ── Locally observed 24h history ───────────────────────────────────────────
// The server does not expose a status timeline, so each browser records what
// it observed on every refresh. Buckets are 15 minutes wide (96 per day).

const HISTORY_KEY = "sp.statusHistory";
const BUCKET_MS = 15 * 60_000;
const BUCKETS = 96;

type History = Record<string, Level>;

function recordHistory(level: Level): History {
  const now = Date.now();
  const bucket = String(Math.floor(now / BUCKET_MS));
  const cutoff = Math.floor(now / BUCKET_MS) - BUCKETS;
  const history = readJson<History>(HISTORY_KEY, {});
  const next: History = {};
  for (const [k, v] of Object.entries(history)) if (Number(k) > cutoff) next[k] = v;
  next[bucket] = next[bucket] ? worst([next[bucket], level]) : level;
  writeJson(HISTORY_KEY, next);
  return next;
}

function HistoryBar({ history }: { history: History }) {
  const current = Math.floor(Date.now() / BUCKET_MS);
  const cells = Array.from({ length: BUCKETS }, (_, i) => current - BUCKETS + 1 + i);
  const observed = cells.filter((b) => history[b]);
  const up = observed.filter((b) => history[b] === "ok").length;
  const w = 100 / BUCKETS;
  return (
    <div className="chart">
      <svg viewBox="0 0 100 10" preserveAspectRatio="none" role="img" aria-label="Observed status over the last 24 hours" style={{ height: 32 }}>
        {cells.map((b, i) => (
          <rect key={b} x={i * w + 0.08} y={0} width={w - 0.16} height={10} rx={0.3} className={`fill-${history[b] ?? "neutral"}`} opacity={history[b] ? 1 : 0.25}>
            <title>{`${new Date(b * BUCKET_MS).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}: ${history[b] ? LEVEL_LABEL[history[b]] : "No data"}`}</title>
          </rect>
        ))}
      </svg>
      <div className="slo-meta muted" style={{ marginTop: 4 }}>
        <span>24h ago</span>
        <span>{observed.length ? `${formatPercent(up / observed.length, 1)} operational (observed by this browser)` : "No observations yet"}</span>
        <span>now</span>
      </div>
    </div>
  );
}

// ── SLO report (admin key) ─────────────────────────────────────────────────

const SLO_LEVEL: Record<string, Level> = { met: "ok", at_risk: "warn", breached: "danger" };

function SloBars({ report }: { report: SloAggregateReport }) {
  if (!report.slos.length) return <div className="empty">No SLOs are being tracked.</div>;
  return (
    <div>
      {report.slos.map((slo) => {
        const level = SLO_LEVEL[slo.status] ?? "neutral";
        const isRatio = slo.sli_type !== "latency";
        const ratio = Number.isFinite(slo.completion_ratio) ? Math.max(0, Math.min(1, slo.completion_ratio)) : 0;
        // Zoom in on the interesting part of the scale for high targets (e.g. 99.9%).
        const floor = isRatio ? Math.max(0, Math.min(ratio, slo.target) - (1 - slo.target) * 4) : 0;
        const scale = (v: number) => ((v - floor) / (1 - floor || 1)) * 100;
        return (
          <div key={slo.name} className="slo-row">
            <div className="slo-meta">
              <span>
                <strong>{slo.name}</strong> <span className="muted">· {slo.component} · {slo.description}</span>
              </span>
              <span className={`badge ${level}`}>{slo.status.replace("_", " ")}</span>
            </div>
            <div className="chart">
              <svg viewBox="0 0 100 6" preserveAspectRatio="none" style={{ height: 12 }} role="img" aria-label={`${slo.name}: ${formatPercent(ratio)} good against target ${isRatio ? formatPercent(slo.target) : `${slo.target}s`}`}>
                <rect className="track" x={0} y={0} width={100} height={6} rx={1} />
                <rect className={`fill-${level}`} x={0} y={0} width={Math.max(0, scale(ratio))} height={6} rx={1} />
                {isRatio && <line className="target" x1={scale(slo.target)} x2={scale(slo.target)} y1={0} y2={6} vectorEffect="non-scaling-stroke" />}
              </svg>
            </div>
            <div className="slo-meta muted">
              <span>
                {formatPercent(ratio)} good · target {isRatio ? formatPercent(slo.target) : `≤ ${slo.target}s`} · window {formatDuration(slo.window_secs)}
              </span>
              <span>
                budget left {formatPercent(slo.error_budget_remaining, 0)} · burn {Number.isFinite(slo.burn_rate) ? slo.burn_rate.toFixed(2) : "—"}×
              </span>
            </div>
          </div>
        );
      })}
    </div>
  );
}

// ── Page ───────────────────────────────────────────────────────────────────

export function StatusPage() {
  const settings = useSettings();
  const [params, setParams] = useSearchParams();
  const publicMode = params.get("public") === "1";
  const now = useNow(1000);

  const tiles = usePolling(
    async () => {
      const t = await loadTiles(publicMode);
      const history = recordHistory(worst(t.map((x) => x.level)));
      return { tiles: t, history };
    },
    REFRESH_MS,
    [publicMode, settings.apiBaseUrl, settings.apiKey],
  );

  const showSlo = !publicMode && !!settings.adminKey;
  const slo = usePolling(
    () => (showSlo ? api<SloAggregateReport>("/v1/admin/slo/report", { auth: "admin" }) : Promise.resolve(null)),
    REFRESH_MS,
    [showSlo, settings.adminKey, settings.apiBaseUrl],
  );

  const list = tiles.data?.tiles ?? [];
  const overall: Level = list.length ? worst(list.map((t) => t.level)) : "neutral";
  const updatedAgo = tiles.lastUpdated ? Math.max(0, Math.round((now - tiles.lastUpdated.getTime()) / 1000)) : null;

  const togglePublic = () => {
    const next = new URLSearchParams(params);
    if (publicMode) next.delete("public");
    else next.set("public", "1");
    setParams(next, { replace: true });
  };

  return (
    <div>
      <div className="page-head">
        <h1>System status</h1>
        <div className="row" style={{ alignItems: "center" }}>
          <span className="muted" aria-live="polite">
            {updatedAgo === null ? "Loading…" : `Last updated ${tiles.lastUpdated!.toLocaleTimeString()} (${updatedAgo}s ago)`} · refreshes every 15s
          </span>
          <button type="button" onClick={() => tiles.refresh()}>
            Refresh
          </button>
          <label className="row" style={{ gap: 6, alignItems: "center" }}>
            <input type="checkbox" checked={publicMode} onChange={togglePublic} />
            Public mode
          </label>
        </div>
      </div>

      <div className={`overall ${overall}`} role="status">
        <span className={`dot ${overall}`} />
        {overall === "neutral"
          ? "Checking systems…"
          : overall === "ok"
            ? "All systems operational"
            : overall === "warn"
              ? "Some systems are degraded"
              : "Major outage — one or more systems are down"}
      </div>

      {publicMode && (
        <p className="muted">
          Public mode shows only non-sensitive, unauthenticated checks. No API keys are sent. Share this page as{" "}
          <code>/status?public=1</code>.
        </p>
      )}

      <div className="grid">
        {list.map((t) => (
          <div key={t.id} className={`tile ${t.level}`}>
            <div className="tile-head">
              <span className={`dot ${t.level}`} aria-hidden />
              {t.name}
              <span className={`tile-state badge ${t.level}`}>{t.summary}</span>
            </div>
            {t.detail && <div className="tile-detail break">{t.detail}</div>}
          </div>
        ))}
      </div>

      <div className="card" style={{ marginTop: 16 }}>
        <div className="card-header">
          <h2>Last 24 hours</h2>
        </div>
        <HistoryBar history={tiles.data?.history ?? {}} />
      </div>

      {showSlo && (
        <div className="card">
          <div className="card-header">
            <h2>Service level objectives</h2>
            {slo.data && <span className="muted">{`${slo.data.counts.met} met · ${slo.data.counts.at_risk} at risk · ${slo.data.counts.breached} breached`}</span>}
          </div>
          {slo.error ? (
            <div className="notice danger">SLO report unavailable: {errorMessage(slo.error)}</div>
          ) : slo.data ? (
            <SloBars report={slo.data} />
          ) : (
            <div className="muted">Loading…</div>
          )}
        </div>
      )}
      {!publicMode && !settings.adminKey && (
        <p className="muted" style={{ marginTop: 16 }}>
          Add an admin key in Settings to see the SLO report.
        </p>
      )}
    </div>
  );
}
