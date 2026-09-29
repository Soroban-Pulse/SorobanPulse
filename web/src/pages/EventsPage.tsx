import { useRef, useCallback, useId } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import { useInfiniteEvents, type Event, type EventType } from "../api/hooks.ts";
import { EventDetailDrawer } from "../components/EventDetailDrawer.tsx";
import { EventExplorerFilters } from "../components/EventExplorerFilters.tsx";
import { ColumnTogglePanel } from "../components/ColumnTogglePanel.tsx";

// ─── Column definitions ───────────────────────────────────────────────────────

export type ColumnId = "ledger" | "time" | "contract" | "type" | "topic0" | "tx_hash";

export interface ColumnDef {
  id: ColumnId;
  label: string;
  defaultVisible: boolean;
}

export const ALL_COLUMNS: ColumnDef[] = [
  { id: "ledger",   label: "Ledger",   defaultVisible: true  },
  { id: "time",     label: "Time",     defaultVisible: true  },
  { id: "contract", label: "Contract", defaultVisible: true  },
  { id: "type",     label: "Type",     defaultVisible: true  },
  { id: "topic0",   label: "Topic 0",  defaultVisible: true  },
  { id: "tx_hash",  label: "Tx Hash",  defaultVisible: true  },
];

// ─── URL ↔ state helpers ──────────────────────────────────────────────────────

/** All filter + UI state that lives in the URL query string. */
export interface ExplorerParams {
  contract_id: string;
  event_type: EventType | "";
  from_ledger: string;
  to_ledger: string;
  topic_sym: string;
  search: string;
  /** Comma-separated list of hidden column ids */
  hidden_cols: string;
  /** "compact" | "comfortable" | "spacious" */
  density: string;
}

const DEFAULTS: ExplorerParams = {
  contract_id: "",
  event_type: "",
  from_ledger: "",
  to_ledger: "",
  topic_sym: "",
  search: "",
  hidden_cols: "",
  density: "comfortable",
};

function readParams(sp: URLSearchParams): ExplorerParams {
  return {
    contract_id: sp.get("contract_id") ?? DEFAULTS.contract_id,
    event_type:  (sp.get("event_type") ?? DEFAULTS.event_type) as EventType | "",
    from_ledger: sp.get("from_ledger") ?? DEFAULTS.from_ledger,
    to_ledger:   sp.get("to_ledger")   ?? DEFAULTS.to_ledger,
    topic_sym:   sp.get("topic_sym")   ?? DEFAULTS.topic_sym,
    search:      sp.get("search")      ?? DEFAULTS.search,
    hidden_cols: sp.get("hidden_cols") ?? DEFAULTS.hidden_cols,
    density:     sp.get("density")     ?? DEFAULTS.density,
  };
}

function buildSearchParams(p: ExplorerParams): URLSearchParams {
  const sp = new URLSearchParams();
  (Object.entries(p) as [keyof ExplorerParams, string][]).forEach(([k, v]) => {
    if (v && v !== DEFAULTS[k]) sp.set(k, v);
  });
  return sp;
}

// ─── Relative-time formatter ──────────────────────────────────────────────────

function relativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const s = Math.floor(diff / 1000);
  if (s < 60)    return `${s}s ago`;
  const m = Math.floor(s / 60);
  if (m < 60)    return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24)    return `${h}h ago`;
  const d = Math.floor(h / 24);
  return `${d}d ago`;
}

// ─── Topic 0 extractor ───────────────────────────────────────────────────────

/**
 * Attempts to read the first topic from the event_data payload.
 * Soroban events typically encode topics as `{ topics: [...], data: ... }`.
 * Falls back to "—" when unavailable.
 */
function extractTopic0(data: unknown): string {
  if (!data || typeof data !== "object") return "—";
  const d = data as Record<string, unknown>;
  const topics = d["topics"] ?? d["topic"] ?? d["Topics"];
  if (Array.isArray(topics) && topics.length > 0) {
    const t0 = topics[0];
    if (typeof t0 === "string") return t0;
    if (typeof t0 === "object" && t0 !== null) {
      // e.g. { "Sym": "transfer" }
      const entries = Object.values(t0 as Record<string, unknown>);
      if (entries.length === 1 && typeof entries[0] === "string") return entries[0] as string;
    }
    try { return JSON.stringify(t0); } catch { return "—"; }
  }
  return "—";
}

// ─── Main component ───────────────────────────────────────────────────────────

export default function EventsPage() {
  const { eventId } = useParams<{ eventId?: string }>();
  const navigate    = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();

  const p = readParams(searchParams);

  // Derived sets
  const hiddenCols = new Set<ColumnId>(
    p.hidden_cols ? (p.hidden_cols.split(",") as ColumnId[]) : [],
  );
  const visibleCols = ALL_COLUMNS.filter((c) => !hiddenCols.has(c.id));

  // Build API query from URL params (strip empties)
  const apiParams = {
    limit: 25,
    ...(p.event_type   ? { event_type:  p.event_type as EventType } : {}),
    ...(p.contract_id  ? { contract_id: p.contract_id }             : {}),
    ...(p.from_ledger  ? { from_ledger: Number(p.from_ledger) }     : {}),
    ...(p.to_ledger    ? { to_ledger:   Number(p.to_ledger) }       : {}),
    ...(p.topic_sym    ? { topic_sym:   p.topic_sym }               : {}),
    ...(p.search       ? { search:      p.search }                  : {}),
  };

  const {
    data,
    isLoading,
    isFetchingNextPage,
    isFetching,
    isError,
    fetchNextPage,
    hasNextPage,
  } = useInfiniteEvents(apiParams);

  const allEvents: Event[] = data?.pages.flatMap((pg) => pg.data) ?? [];
  const totalApprox: number = data?.pages[0]?.total ?? 0;

  // ── Selected event (drawer) ────────────────────────────────────────────────

  // When a deep-link eventId arrives find the event in loaded pages.
  const selectedEvent = eventId
    ? (allEvents.find((e) => e.id === eventId) ?? null)
    : null;

  function openDrawer(event: Event) {
    navigate(`/events/${event.id}`, { replace: true });
  }

  function closeDrawer() {
    navigate("/events?" + searchParams.toString(), { replace: true });
  }

  function handleRelatedNavigate(newId: string) {
    navigate(`/events/${newId}`, { replace: true });
  }

  // ── Filter update helper ───────────────────────────────────────────────────

  function setParam(key: keyof ExplorerParams, value: string) {
    const next = { ...p, [key]: value };
    setSearchParams(buildSearchParams(next), { replace: true });
  }

  function resetFilters() {
    const next: ExplorerParams = {
      ...DEFAULTS,
      hidden_cols: p.hidden_cols,
      density:     p.density,
    };
    setSearchParams(buildSearchParams(next), { replace: true });
  }

  const hasActiveFilters =
    !!(p.contract_id || p.event_type || p.from_ledger || p.to_ledger || p.topic_sym || p.search);

  // ── Column visibility ──────────────────────────────────────────────────────

  function toggleColumn(id: ColumnId) {
    const next = new Set(hiddenCols);
    if (next.has(id)) next.delete(id);
    else              next.add(id);
    setParam("hidden_cols", [...next].join(","));
  }

  // ── Keyboard navigation ────────────────────────────────────────────────────

  const tbodyRef = useRef<HTMLTableSectionElement>(null);

  const handleRowKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTableRowElement>, event: Event) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        openDrawer(event);
        return;
      }

      const rows = tbodyRef.current
        ? Array.from(tbodyRef.current.querySelectorAll<HTMLTableRowElement>("tr[tabindex='0']"))
        : [];
      const idx = rows.indexOf(e.currentTarget);

      if (e.key === "ArrowDown") {
        e.preventDefault();
        rows[idx + 1]?.focus();
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        rows[idx - 1]?.focus();
      } else if (e.key === "Home") {
        e.preventDefault();
        rows[0]?.focus();
      } else if (e.key === "End") {
        e.preventDefault();
        rows[rows.length - 1]?.focus();
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );

  // Auto-focus the "Load more" button when it comes into view (convenience).
  const loadMoreRef = useRef<HTMLButtonElement>(null);

  // ── Announce filter result count for screen readers ───────────────────────

  const announceId = useId();

  // ── Render ─────────────────────────────────────────────────────────────────

  return (
    <>
      <div>
        <div className="page-header">
          <h1>Event Explorer</h1>
          <p>Browse and filter all indexed Soroban contract events.</p>
        </div>

        {/* ── Filter bar ── */}
        <EventExplorerFilters
          params={p}
          hasActiveFilters={hasActiveFilters}
          onChange={setParam}
          onReset={resetFilters}
        />

        {/* ── Table toolbar (column toggles + density) ── */}
        <div className="explorer-toolbar">
          <span
            id={announceId}
            className="explorer-toolbar__count text-muted"
            aria-live="polite"
            aria-atomic="true"
          >
            {isLoading
              ? "Loading…"
              : `${totalApprox.toLocaleString()} events${hasActiveFilters ? " (filtered)" : ""}`}
          </span>

          <div className="explorer-toolbar__right">
            {/* Density selector */}
            <label className="explorer-density-label">
              <span className="sr-only">Row density</span>
              <select
                className="explorer-density-select"
                value={p.density}
                onChange={(e) => setParam("density", e.target.value)}
                aria-label="Row density"
              >
                <option value="compact">Compact</option>
                <option value="comfortable">Comfortable</option>
                <option value="spacious">Spacious</option>
              </select>
            </label>

            {/* Column visibility toggle */}
            <ColumnTogglePanel
              columns={ALL_COLUMNS}
              hiddenCols={hiddenCols}
              onToggle={toggleColumn}
            />
          </div>
        </div>

        {/* ── Events table ── */}
        <div className="card" style={{ padding: 0 }}>
          {isError && (
            <div className="error-banner" style={{ margin: "1rem" }}>
              ⚠ Failed to load events
            </div>
          )}

          <div className="table-wrap">
            <table
              role="grid"
              aria-label="Events"
              aria-describedby={announceId}
              className={`explorer-table explorer-table--${p.density}`}
            >
              <thead>
                <tr role="row">
                  {visibleCols.map((col) => (
                    <th key={col.id} data-col={col.id}>
                      {col.label}
                    </th>
                  ))}
                </tr>
              </thead>

              <tbody ref={tbodyRef}>
                {isLoading && (
                  <tr>
                    <td colSpan={visibleCols.length} className="explorer-table__status-cell">
                      <span className="text-muted">Loading events…</span>
                    </td>
                  </tr>
                )}

                {!isLoading && allEvents.length === 0 && (
                  <tr>
                    <td colSpan={visibleCols.length} className="explorer-table__status-cell">
                      <div className="empty-state">
                        <div className="empty-state__icon">🔍</div>
                        <p>No events match the current filters.</p>
                        {hasActiveFilters && (
                          <button className="btn mt-2" onClick={resetFilters}>
                            Clear filters
                          </button>
                        )}
                      </div>
                    </td>
                  </tr>
                )}

                {allEvents.map((e) => {
                  const isActive = selectedEvent?.id === e.id;
                  const absTime  = new Date(e.timestamp).toLocaleString();
                  const relTime  = relativeTime(e.timestamp);
                  const topic0   = extractTopic0(e.event_data);

                  return (
                    <tr
                      key={e.id}
                      role="row"
                      className={`row--clickable${isActive ? " row--active" : ""}`}
                      style={{ opacity: isFetching && !isFetchingNextPage ? 0.6 : 1 }}
                      onClick={() => openDrawer(e)}
                      tabIndex={0}
                      aria-selected={isActive}
                      aria-label={`Event ${e.id.slice(0, 8)}, ${e.event_type}, ledger ${e.ledger}`}
                      onKeyDown={(ev) => handleRowKeyDown(ev, e)}
                    >
                      {!hiddenCols.has("ledger") && (
                        <td data-col="ledger">{e.ledger.toLocaleString()}</td>
                      )}
                      {!hiddenCols.has("time") && (
                        <td data-col="time">
                          <span
                            className="explorer-reltime"
                            title={absTime}
                            aria-label={absTime}
                          >
                            {relTime}
                          </span>
                        </td>
                      )}
                      {!hiddenCols.has("contract") && (
                        <td data-col="contract" className="mono" title={e.contract_id}>
                          {e.contract_id.slice(0, 14)}…
                        </td>
                      )}
                      {!hiddenCols.has("type") && (
                        <td data-col="type">
                          <span className={`badge badge--${e.event_type}`}>
                            {e.event_type}
                          </span>
                        </td>
                      )}
                      {!hiddenCols.has("topic0") && (
                        <td data-col="topic0" className="mono explorer-topic0" title={topic0}>
                          {topic0.length > 24 ? topic0.slice(0, 24) + "…" : topic0}
                        </td>
                      )}
                      {!hiddenCols.has("tx_hash") && (
                        <td data-col="tx_hash" className="mono" title={e.tx_hash}>
                          {e.tx_hash.slice(0, 10)}…
                        </td>
                      )}
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>

          {/* ── Load more ── */}
          {!isLoading && allEvents.length > 0 && (
            <div className="explorer-load-more">
              {hasNextPage ? (
                <button
                  ref={loadMoreRef}
                  className="btn"
                  onClick={() => fetchNextPage()}
                  disabled={isFetchingNextPage}
                  aria-label="Load more events"
                >
                  {isFetchingNextPage ? "Loading…" : "Load more"}
                </button>
              ) : (
                <span className="text-muted" style={{ fontSize: "0.8125rem" }}>
                  All {allEvents.length.toLocaleString()} events loaded
                </span>
              )}
            </div>
          )}
        </div>
      </div>

      {/* Detail drawer */}
      {selectedEvent && (
        <EventDetailDrawer
          event={selectedEvent}
          onClose={closeDrawer}
          onNavigate={handleRelatedNavigate}
        />
      )}
    </>
  );
}
