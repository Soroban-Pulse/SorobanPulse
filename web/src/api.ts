// Thin client for the SorobanPulse REST + SSE API.

export interface PulseEvent {
  id?: string;
  contractId: string;
  type: string;
  txHash: string;
  ledger: number;
  timestamp: string;
  value: unknown;
  raw: Record<string, unknown>;
}

export interface Settings {
  /** API origin; empty string means "same origin as the dashboard". */
  server: string;
  apiKey: string;
}

const SETTINGS_KEY = 'sp.dashboard.settings';

export function loadSettings(): Settings {
  try {
    const s = JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? '{}');
    return { server: s.server ?? '', apiKey: s.apiKey ?? '' };
  } catch {
    return { server: '', apiKey: '' };
  }
}

export function saveSettings(s: Settings): void {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(s));
  } catch {
    /* storage unavailable — settings last for this session only */
  }
}

/** REST rows use snake_case columns; SSE frames use the camelCase SorobanEvent. */
export function normalize(raw: Record<string, any>): PulseEvent {
  return {
    id: raw.id,
    contractId: raw.contract_id ?? raw.contractId ?? '',
    type: String(raw.event_type ?? raw.type ?? 'contract').toLowerCase(),
    txHash: raw.tx_hash ?? raw.txHash ?? '',
    ledger: Number(raw.ledger ?? 0),
    timestamp: raw.timestamp ?? raw.ledgerClosedAt ?? raw.created_at ?? '',
    value: raw.event_data ?? raw.value ?? null,
    raw,
  };
}

export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
  }
}

export class PulseClient {
  constructor(private settings: Settings) {}

  private url(path: string, params: Record<string, string | number | undefined> = {}): string {
    const base = this.settings.server.replace(/\/+$/, '') || location.origin;
    const u = new URL(path, base + '/');
    for (const [k, v] of Object.entries(params)) {
      if (v !== undefined && v !== '') u.searchParams.set(k, String(v));
    }
    return u.toString();
  }

  private headers(extra: Record<string, string> = {}): HeadersInit {
    return this.settings.apiKey ? { 'X-Api-Key': this.settings.apiKey, ...extra } : extra;
  }

  async events(q: { contractId?: string; eventType?: string; page: number; limit: number }): Promise<PulseEvent[]> {
    const path = q.contractId ? `v1/events/contract/${encodeURIComponent(q.contractId)}` : 'v1/events';
    const res = await fetch(
      this.url(path, { page: q.page, limit: q.limit, sort: 'desc', event_type: q.eventType }),
      { headers: this.headers({ Accept: 'application/json' }) },
    );
    // The per-contract endpoint answers 404 when the contract has no events yet.
    if (res.status === 404 && q.contractId) return [];
    if (!res.ok) throw new ApiError(res.status, await errorMessage(res));
    const body = await res.json();
    return (Array.isArray(body) ? body : body.data ?? []).map(normalize);
  }

  /**
   * Subscribe to the SSE stream. Uses fetch rather than EventSource so the API
   * key can travel in a header instead of the URL. Reconnects with backoff and
   * resumes from the last event ID. Returns an unsubscribe function.
   */
  stream(
    q: { contractId?: string; eventType?: string },
    onEvent: (e: PulseEvent) => void,
    onState: (s: 'connecting' | 'open' | 'error') => void,
  ): () => void {
    const ctrl = new AbortController();
    let lastId = '';
    let delay = 1000;

    const run = async (): Promise<void> => {
      while (!ctrl.signal.aborted) {
        onState('connecting');
        try {
          const path = q.contractId
            ? `v1/events/contract/${encodeURIComponent(q.contractId)}/stream`
            : 'v1/events/stream';
          const headers: Record<string, string> = { Accept: 'text/event-stream' };
          if (lastId) headers['Last-Event-ID'] = lastId;
          const res = await fetch(this.url(path, { event_type: q.eventType }), {
            headers: this.headers(headers),
            signal: ctrl.signal,
          });
          if (!res.ok || !res.body) throw new ApiError(res.status, await errorMessage(res));
          onState('open');
          delay = 1000;
          await readSse(res.body, (id, event, data) => {
            if (id) lastId = id;
            if (event && event !== 'message') return;
            try {
              onEvent(normalize(JSON.parse(data)));
            } catch {
              /* keep-alive comments and non-JSON frames are ignored */
            }
          });
        } catch (err) {
          if (ctrl.signal.aborted) return;
          onState('error');
          if (err instanceof ApiError && (err.status === 401 || err.status === 403)) return;
        }
        await new Promise((r) => setTimeout(r, delay));
        delay = Math.min(delay * 2, 30_000);
      }
    };
    void run();
    return () => ctrl.abort();
  }
}

async function errorMessage(res: Response): Promise<string> {
  try {
    const body = await res.json();
    return body.error ?? body.message ?? `${res.status} ${res.statusText}`;
  } catch {
    return `${res.status} ${res.statusText}`;
  }
}

async function readSse(
  body: ReadableStream<Uint8Array>,
  onFrame: (id: string, event: string, data: string) => void,
): Promise<void> {
  const reader = body.pipeThrough(new TextDecoderStream()).getReader();
  let buf = '';
  for (;;) {
    const { value, done } = await reader.read();
    if (done) return;
    buf += value;
    let idx: number;
    while ((idx = buf.search(/\r?\n\r?\n/)) >= 0) {
      const frame = buf.slice(0, idx);
      buf = buf.slice(idx).replace(/^\r?\n\r?\n/, '');
      let id = '';
      let event = '';
      const data: string[] = [];
      for (const line of frame.split(/\r?\n/)) {
        const colon = line.indexOf(':');
        if (colon === 0) continue;
        const field = colon < 0 ? line : line.slice(0, colon);
        const val = colon < 0 ? '' : line.slice(colon + 1).replace(/^ /, '');
        if (field === 'id') id = val;
        else if (field === 'event') event = val;
        else if (field === 'data') data.push(val);
      }
      if (data.length) onFrame(id, event, data.join('\n'));
    }
  }
}
