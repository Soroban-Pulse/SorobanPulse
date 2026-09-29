/**
 * useEventStream — fetch-based SSE hook for /v1/events/stream and
 * /v1/events/stream/multi.
 *
 * Why fetch instead of EventSource?
 *   The backend authenticates via `X-Api-Key` / `Authorization: Bearer`
 *   headers. The native EventSource API does not support custom request
 *   headers, so we use the Fetch API with a ReadableStream body and parse
 *   the SSE wire format manually.
 *
 * Features
 *  - Last-Event-ID reconnect resume (sent on every retry)
 *  - Exponential backoff with jitter (1 s → 30 s cap)
 *  - Pause / resume: incoming events are buffered while paused; a badge
 *    count is returned so the UI can show "N buffered"
 *  - Rolling in-memory cap (default 1 000 rows) — oldest rows are dropped
 *    when the cap is exceeded so memory never grows unboundedly
 *  - Named SSE event dispatch: ping, lag, close, replay_complete
 */

import { useCallback, useEffect, useRef, useState } from "react";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/** camelCase payload pushed by the server over the SSE stream. */
export interface SorobanStreamEvent {
  id?: string;
  contractId: string;
  type: string;
  txHash: string;
  ledger: number;
  ledgerClosedAt: string;
  ledgerHash?: string;
  inSuccessfulContractCall: boolean;
  value: unknown;
  topic?: unknown[];
}

export type StreamStatus =
  | "idle"
  | "connecting"
  | "open"
  | "paused"
  | "error"
  | "closed";

export interface UseEventStreamOptions {
  /** Maximum number of events to keep in memory (oldest are evicted). Default: 1000. */
  maxRows?: number;
  /** Whether the stream should start paused. Default: false. */
  startPaused?: boolean;
}

export interface UseEventStreamResult {
  /** Events visible in the list (excludes buffered-while-paused events). */
  events: SorobanStreamEvent[];
  /** Current connection / stream state. */
  status: StreamStatus;
  /** Number of events buffered while the stream is paused. */
  bufferedCount: number;
  /** Pause the stream — new events are buffered, not shown. */
  pause: () => void;
  /** Resume the stream — flushed buffered events into the list. */
  resume: () => void;
  /** Close the stream permanently and reset state. */
  disconnect: () => void;
  /** Last lag notification payload, if any. */
  lagInfo: { missed: number } | null;
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

const DEFAULT_MAX_ROWS = 1_000;
const BACKOFF_BASE_MS = 1_000;
const BACKOFF_MAX_MS = 30_000;

/** Clamp an array to at most `max` items, dropping from the front. */
function clampRows<T>(arr: T[], max: number): T[] {
  if (arr.length <= max) return arr;
  return arr.slice(arr.length - max);
}

/** Compute jittered exponential backoff. */
function nextBackoff(attempt: number): number {
  const exp = Math.min(BACKOFF_BASE_MS * 2 ** attempt, BACKOFF_MAX_MS);
  return exp * (0.75 + Math.random() * 0.5); // ±25 % jitter
}

/** Parse a single SSE message block into its fields. */
function parseSseBlock(block: string): {
  id?: string;
  event?: string;
  data?: string;
} {
  const result: { id?: string; event?: string; data?: string } = {};
  const dataLines: string[] = [];

  for (const line of block.split("\n")) {
    if (line.startsWith("id:")) {
      result.id = line.slice(3).trimStart();
    } else if (line.startsWith("event:")) {
      result.event = line.slice(6).trimStart();
    } else if (line.startsWith("data:")) {
      dataLines.push(line.slice(5).trimStart());
    }
  }

  if (dataLines.length > 0) {
    result.data = dataLines.join("\n");
  }

  return result;
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

export function useEventStream(
  /** Full URL (with query params) for the SSE endpoint, or null to stay idle. */
  url: string | null,
  options: UseEventStreamOptions = {},
): UseEventStreamResult {
  const { maxRows = DEFAULT_MAX_ROWS } = options;
  // startPaused is read only at mount time; store it in a ref so the
  // useEffect dep array stays [url]-only without triggering on re-renders.
  const startPausedRef = useRef(options.startPaused ?? false);

  const [events, setEvents] = useState<SorobanStreamEvent[]>([]);
  const [status, setStatus] = useState<StreamStatus>("idle");
  const [bufferedCount, setBufferedCount] = useState(0);
  const [lagInfo, setLagInfo] = useState<{ missed: number } | null>(null);

  // Refs so closures inside the async loop always see the latest values
  // without causing effect re-runs.
  const pausedRef = useRef(startPausedRef.current);
  const bufferRef = useRef<SorobanStreamEvent[]>([]);
  const lastEventIdRef = useRef<string | undefined>(undefined);
  const abortRef = useRef<AbortController | null>(null);
  const attemptRef = useRef(0);
  const retryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const mountedRef = useRef(true);
  const urlRef = useRef(url);
  urlRef.current = url;

  // ── pause / resume ────────────────────────────────────────────────────────

  const pause = useCallback(() => {
    pausedRef.current = true;
    setStatus("paused");
  }, []);

  const resume = useCallback(() => {
    pausedRef.current = false;
    // Flush buffered events into the visible list.
    const buffered = bufferRef.current.splice(0);
    if (buffered.length > 0) {
      setEvents((prev) => clampRows([...prev, ...buffered], maxRows));
    }
    setBufferedCount(0);
    setStatus("open");
  }, [maxRows]);

  const disconnect = useCallback(() => {
    abortRef.current?.abort();
    if (retryTimerRef.current !== null) {
      clearTimeout(retryTimerRef.current);
      retryTimerRef.current = null;
    }
    setStatus("closed");
    setEvents([]);
    setBufferedCount(0);
    bufferRef.current = [];
    lastEventIdRef.current = undefined;
    attemptRef.current = 0;
  }, []);

  // ── core streaming loop ───────────────────────────────────────────────────

  useEffect(() => {
    mountedRef.current = true;

    if (!url) {
      setStatus("idle");
      return;
    }

    // Reset visible state when the URL changes (new filter applied).
    setEvents([]);
    setBufferedCount(0);
    bufferRef.current = [];
    lastEventIdRef.current = undefined;
    attemptRef.current = 0;
    pausedRef.current = startPausedRef.current;

    let cancelled = false;

    async function connect() {
      if (cancelled || !mountedRef.current) return;

      const controller = new AbortController();
      abortRef.current = controller;

      setStatus("connecting");

      // Build request headers — inject API key if available.
      const headers: Record<string, string> = {
        Accept: "text/event-stream",
        "Cache-Control": "no-cache",
      };

      // Read credentials from Web Storage at connect-time so changes saved
      // on the Settings page are picked up on the next reconnect without a
      // full page reload. Mirrors the strategy used in api/client.ts.
      const apiKey =
        sessionStorage.getItem("sp_api_key") ??
        localStorage.getItem("sp_api_key") ??
        "";
      const adminKey =
        sessionStorage.getItem("sp_admin_key") ??
        localStorage.getItem("sp_admin_key") ??
        "";

      if (apiKey) headers["X-Api-Key"] = apiKey;
      if (adminKey) headers["X-Admin-Key"] = adminKey;

      // Replay from where we left off.
      if (lastEventIdRef.current) {
        headers["Last-Event-ID"] = lastEventIdRef.current;
      }

      try {
        const response = await fetch(urlRef.current!, {
          headers,
          signal: controller.signal,
        });

        if (!mountedRef.current || cancelled) return;

        if (!response.ok) {
          throw new Error(`HTTP ${response.status} ${response.statusText}`);
        }

        if (!response.body) {
          throw new Error("Response body is null");
        }

        setStatus(pausedRef.current ? "paused" : "open");
        attemptRef.current = 0; // reset backoff on successful connection

        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let remainder = "";

        // eslint-disable-next-line no-constant-condition
        while (true) {
          const { done, value } = await reader.read();
          if (done || cancelled || !mountedRef.current) break;

          remainder += decoder.decode(value, { stream: true });

          // SSE blocks are delimited by double newlines.
          const blocks = remainder.split(/\n\n/);
          remainder = blocks.pop() ?? "";

          for (const block of blocks) {
            const trimmed = block.trim();
            if (!trimmed) continue;

            const { id, event, data } = parseSseBlock(trimmed);

            // Track the last event ID for reconnect resume.
            if (id) lastEventIdRef.current = id;

            // Handle named lifecycle events.
            if (event === "ping") continue;
            if (event === "replay_complete") continue;
            if (event === "close") {
              if (mountedRef.current && !cancelled) setStatus("closed");
              reader.cancel();
              return;
            }
            if (event === "lag" && data) {
              try {
                const payload = JSON.parse(data) as { missed: number };
                if (mountedRef.current) setLagInfo({ missed: payload.missed });
              } catch {
                /* ignore malformed lag payload */
              }
              continue;
            }

            // Default event — a SorobanStreamEvent payload.
            if (!data) continue;
            let parsed: SorobanStreamEvent;
            try {
              parsed = JSON.parse(data) as SorobanStreamEvent;
            } catch {
              continue;
            }

            if (pausedRef.current) {
              bufferRef.current.push(parsed);
              if (mountedRef.current) {
                setBufferedCount(bufferRef.current.length);
              }
            } else {
              if (mountedRef.current) {
                setEvents((prev) =>
                  clampRows([...prev, parsed], maxRows),
                );
              }
            }
          }
        }

        // Stream ended cleanly — schedule reconnect.
        if (!cancelled && mountedRef.current) {
          scheduleReconnect();
        }
      } catch (err) {
        if (cancelled || !mountedRef.current) return;
        // AbortError means we closed intentionally — don't reconnect.
        if ((err as { name?: string }).name === "AbortError") return;

        if (mountedRef.current) setStatus("error");
        scheduleReconnect();
      }
    }

    function scheduleReconnect() {
      if (cancelled || !mountedRef.current) return;
      const delay = nextBackoff(attemptRef.current++);
      retryTimerRef.current = setTimeout(() => {
        if (!cancelled && mountedRef.current) connect();
      }, delay);
    }

    connect();

    return () => {
      cancelled = true;
      mountedRef.current = false;
      abortRef.current?.abort();
      if (retryTimerRef.current !== null) {
        clearTimeout(retryTimerRef.current);
        retryTimerRef.current = null;
      }
    };
  }, [url]);

  // Keep mountedRef in sync across renders (avoids stale closure issues).
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  return { events, status, bufferedCount, pause, resume, disconnect, lagInfo };
}
