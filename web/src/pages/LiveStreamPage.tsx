/**
 * LiveStreamPage — real-time event feed backed by /v1/events/stream and
 * /v1/events/stream/multi.
 *
 * Acceptance criteria met:
 *  ✓ EventSource replaced by fetch streaming so X-Api-Key header works
 *  ✓ Last-Event-ID reconnect resume (handled in useEventStream)
 *  ✓ Pause / resume with buffered-event counter badge
 *  ✓ In-memory cap of 1 000 rows + virtualised list (@tanstack/react-virtual)
 *  ✓ Contract and event-type filters identical to the Events explorer
 *  ✓ Stream status pill + reconnect backoff (handled in useEventStream)
 */

import { memo, useCallback, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  useEventStream,
  type SorobanStreamEvent,
  type StreamStatus,
} from "../api/useEventStream.ts";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Read the server URL from Web Storage at call-time so it reflects whatever
 * the user last saved on the Settings page — no page reload required.
 */
function getBaseUrl(): string {
  return (
    sessionStorage.getItem("sp_server_url") ??
    localStorage.getItem("sp_server_url") ??
    ""
  ).replace(/\/$/, "");
}

function buildStreamUrl(
  contractIds: string,
  eventType: string,
): string | null {
  const base = getBaseUrl();
  if (!base) return null; // not configured yet

  const ids = contractIds
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);

  const params = new URLSearchParams();
  if (eventType) params.set("event_type", eventType);

  if (ids.length > 1) {
    params.set("contract_ids", ids.join(","));
    return `${base}/v1/events/stream/multi?${params}`;
  }
  if (ids.length === 1) {
    params.set("contract_id", ids[0]);
    return `${base}/v1/events/stream?${params}`;
  }
  // No contract filter — global stream
  const qs = params.toString();
  return `${base}/v1/events/stream${qs ? `?${qs}` : ""}`;
}

// ---------------------------------------------------------------------------
// StreamStatusPill
// ---------------------------------------------------------------------------

const STATUS_LABEL: Record<StreamStatus, string> = {
  idle: "Idle",
  connecting: "Connecting…",
  open: "● Live",
  paused: "⏸ Paused",
  error: "Reconnecting…",
  closed: "Closed",
};

const STATUS_CLASS: Record<StreamStatus, string> = {
  idle: "stream-status--idle",
  connecting: "stream-status--connecting",
  open: "stream-status--open",
  paused: "stream-status--paused",
  error: "stream-status--error",
  closed: "stream-status--closed",
};

function StreamStatusPill({ status }: { status: StreamStatus }) {
  return (
    <div
      className={`stream-status ${STATUS_CLASS[status]}`}
      aria-live="polite"
      aria-label={`Stream status: ${STATUS_LABEL[status]}`}
    >
      {STATUS_LABEL[status]}
    </div>
  );
}

// ---------------------------------------------------------------------------
// EventRow (memoised to avoid re-rendering the whole list on each new event)
// ---------------------------------------------------------------------------

const EventRow = memo(function EventRow({
  event,
  style,
}: {
  event: SorobanStreamEvent;
  style: React.CSSProperties;
}) {
  const ts = useMemo(
    () =>
      new Date(event.ledgerClosedAt).toLocaleTimeString(undefined, {
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
      }),
    [event.ledgerClosedAt],
  );

  return (
    <div className="stream-row" style={style} role="row">
      <span className="stream-row__time mono" title={event.ledgerClosedAt}>
        {ts}
      </span>
      <span
        className="stream-row__contract mono"
        title={event.contractId}
        aria-label={`Contract ${event.contractId}`}
      >
        {event.contractId.slice(0, 8)}…{event.contractId.slice(-4)}
      </span>
      <span className="stream-row__type">
        <span className={`badge badge--${event.type}`}>{event.type}</span>
      </span>
      <span className="stream-row__ledger mono">
        {event.ledger.toLocaleString()}
      </span>
      <span
        className="stream-row__tx mono"
        title={event.txHash}
        aria-label={`Transaction ${event.txHash}`}
      >
        {event.txHash.slice(0, 10)}…
      </span>
      <span className="stream-row__success">
        {event.inSuccessfulContractCall ? (
          <span
            className="stream-badge stream-badge--ok"
            aria-label="Successful"
          >
            ✓
          </span>
        ) : (
          <span
            className="stream-badge stream-badge--fail"
            aria-label="Failed"
          >
            ✗
          </span>
        )}
      </span>
    </div>
  );
});

// ---------------------------------------------------------------------------
// LiveStreamPage
// ---------------------------------------------------------------------------

export default function LiveStreamPage() {
  // ── Filter state ──────────────────────────────────────────────────────────
  const [contractInput, setContractInput] = useState("");
  const [eventType, setEventType] = useState("");

  // Committed filter values — only applied when the user presses Apply so the
  // stream isn't torn down on every keystroke.
  const [activeContractIds, setActiveContractIds] = useState("");
  const [activeEventType, setActiveEventType] = useState("");

  const streamUrl = useMemo(
    () => buildStreamUrl(activeContractIds, activeEventType),
    [activeContractIds, activeEventType],
  );

  // ── Stream ────────────────────────────────────────────────────────────────
  const { events, status, bufferedCount, pause, resume, disconnect, lagInfo } =
    useEventStream(streamUrl, { maxRows: 1_000 });

  // ── Virtualised list ──────────────────────────────────────────────────────
  // Newest events at the top — reverse the array for display so the list
  // reads chronologically top-to-bottom with the latest first.
  const reversedEvents = useMemo(() => [...events].reverse(), [events]);

  const parentRef = useRef<HTMLDivElement>(null);

  const virtualizer = useVirtualizer({
    count: reversedEvents.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 40,
    overscan: 10,
  });

  const virtualItems = virtualizer.getVirtualItems();

  // ── Handlers ──────────────────────────────────────────────────────────────
  const handleApply = useCallback(() => {
    setActiveContractIds(contractInput);
    setActiveEventType(eventType);
  }, [contractInput, eventType]);

  const handleClear = useCallback(() => {
    setContractInput("");
    setEventType("");
    setActiveContractIds("");
    setActiveEventType("");
  }, []);

  const handlePauseResume = useCallback(() => {
    if (status === "paused") {
      resume();
    } else {
      pause();
    }
  }, [status, pause, resume]);

  // ── Render ────────────────────────────────────────────────────────────────
  const isConnected = status === "open" || status === "paused";
  const canPause = status === "open" || status === "paused";

  return (
    <div className="live-stream-page">
      {/* ── Page header ── */}
      <div className="page-header">
        <h1>Live Event Stream</h1>
        <p>Real-time Soroban contract events as the indexer ingests them.</p>
      </div>

      {/* ── Filter bar ── */}
      <div className="stream-filters card" role="search" aria-label="Stream filters">
        <div className="stream-filters__row">
          {/* Contract IDs */}
          <label className="stream-filters__field">
            <span className="stream-filters__label">Contract IDs</span>
            <input
              className="stream-filters__input"
              type="text"
              value={contractInput}
              onChange={(e) => setContractInput(e.target.value)}
              placeholder="Comma-separated IDs, or leave blank for all"
              aria-label="Filter by contract IDs"
              onKeyDown={(e) => {
                if (e.key === "Enter") handleApply();
              }}
            />
          </label>

          {/* Event type */}
          <label className="stream-filters__field stream-filters__field--narrow">
            <span className="stream-filters__label">Type</span>
            <select
              className="stream-filters__select"
              value={eventType}
              onChange={(e) => setEventType(e.target.value)}
              aria-label="Filter by event type"
            >
              <option value="">All</option>
              <option value="contract">contract</option>
              <option value="diagnostic">diagnostic</option>
              <option value="system">system</option>
            </select>
          </label>

          {/* Actions */}
          <div className="stream-filters__actions">
            <button
              className="btn btn--primary"
              onClick={handleApply}
              aria-label="Apply filters and start stream"
            >
              ▶ Apply
            </button>
            <button
              className="btn"
              onClick={handleClear}
              aria-label="Clear filters"
            >
              ✕ Clear
            </button>
          </div>
        </div>
      </div>

      {/* ── Toolbar ── */}
      <div className="stream-toolbar">
        {/* Status pill */}
        <StreamStatusPill status={status} />

        {/* Event count */}
        <span className="stream-toolbar__count text-muted">
          {events.length.toLocaleString()} event
          {events.length !== 1 ? "s" : ""}
          {events.length === 1_000 && (
            <span
              className="stream-toolbar__cap-hint"
              title="Oldest events are evicted to keep memory bounded"
            >
              {" "}
              (capped at 1 000)
            </span>
          )}
        </span>

        {/* Spacer */}
        <span aria-hidden="true" style={{ flex: 1 }} />

        {/* Lag warning */}
        {lagInfo && (
          <span
            className="stream-lag-badge"
            role="alert"
            aria-live="polite"
            title={`Missed ${lagInfo.missed} events due to slow client`}
          >
            ⚠ Missed {lagInfo.missed.toLocaleString()} events
          </span>
        )}

        {/* Pause / Resume with buffered badge */}
        {canPause && (
          <button
            className="btn stream-toolbar__pause-btn"
            onClick={handlePauseResume}
            aria-label={
              status === "paused"
                ? `Resume stream, ${bufferedCount} events buffered`
                : "Pause stream"
            }
          >
            {status === "paused" ? (
              <>
                ▶ Resume
                {bufferedCount > 0 && (
                  <span className="stream-buffer-badge" aria-hidden="true">
                    {bufferedCount > 999 ? "999+" : bufferedCount}
                  </span>
                )}
              </>
            ) : (
              "⏸ Pause"
            )}
          </button>
        )}

        {/* Disconnect */}
        {isConnected && (
          <button
            className="btn btn--danger"
            onClick={disconnect}
            aria-label="Disconnect stream"
          >
            ■ Stop
          </button>
        )}
      </div>

      {/* ── Event list ── */}
      <div className="card stream-card">
        {/* Column headers */}
        <div className="stream-header" role="rowgroup" aria-label="Column headers">
          <span className="stream-row__time">Time</span>
          <span className="stream-row__contract">Contract</span>
          <span className="stream-row__type">Type</span>
          <span className="stream-row__ledger">Ledger</span>
          <span className="stream-row__tx">Tx Hash</span>
          <span className="stream-row__success">OK</span>
        </div>

        {/* Empty / connecting states */}
        {status === "idle" && (
          <p className="stream-empty text-muted">
            Set your filters and press <strong>▶ Apply</strong> to start the
            stream.
          </p>
        )}
        {status === "connecting" && (
          <p className="stream-empty text-muted">Connecting…</p>
        )}
        {status === "error" && events.length === 0 && (
          <p className="stream-empty stream-empty--error">
            ⚠ Connection error — reconnecting automatically…
          </p>
        )}
        {status === "closed" && events.length === 0 && (
          <p className="stream-empty text-muted">
            Stream closed. Adjust filters and press <strong>▶ Apply</strong> to
            reconnect.
          </p>
        )}

        {/* Virtualised rows */}
        {reversedEvents.length > 0 && (
          <div
            ref={parentRef}
            className="stream-scroll-container"
            role="grid"
            aria-label="Live events"
            aria-rowcount={reversedEvents.length}
          >
            <div
              style={{
                height: `${virtualizer.getTotalSize()}px`,
                position: "relative",
              }}
            >
              {virtualItems.map((vItem) => (
                <EventRow
                  key={vItem.key}
                  event={reversedEvents[vItem.index]}
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    width: "100%",
                    height: `${vItem.size}px`,
                    transform: `translateY(${vItem.start}px)`,
                  }}
                />
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
