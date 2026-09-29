/**
 * <soroban-pulse-feed> — embeddable "recent activity" feed for a Soroban contract.
 *
 *   <script type="module" src="https://cdn.jsdelivr.net/npm/@soroban-pulse/feed-widget@0/dist/feed-widget.js"></script>
 *   <soroban-pulse-feed server="https://pulse.example.com" contract-id="C…" limit="10" live></soroban-pulse-feed>
 *
 * Attributes: server, contract-id, limit, theme (auto|light|dark), live,
 * api-key, explorer-url (template containing {tx}), heading.
 * Events: `sp-feed-event` (detail: FeedEvent), `sp-feed-error` (detail: { message, status }).
 */

export interface FeedEvent {
  id?: string;
  contractId: string;
  type: string;
  txHash: string;
  ledger: number;
  timestamp: string;
  topics: string[];
  raw: Record<string, unknown>;
}

const MAX_LIMIT = 50;

const STYLES = /* css */ `
:host{
  --sp-feed-bg:#fff;--sp-feed-fg:#0f1222;--sp-feed-muted:#6b7187;--sp-feed-border:#dde1ea;
  --sp-feed-hover:#f7f8fb;--sp-feed-accent:#5b3df5;--sp-feed-accent-bg:#f3f0ff;
  --sp-feed-system:#0b6b7d;--sp-feed-diagnostic:#92400e;--sp-feed-live:#15803d;--sp-feed-error:#b91c1c;
  --sp-feed-radius:10px;--sp-feed-max-height:none;
  --sp-feed-font:Inter,ui-sans-serif,system-ui,-apple-system,"Segoe UI",Roboto,sans-serif;
  --sp-feed-font-mono:"JetBrains Mono",ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;
  display:block;color-scheme:light
}
:host([hidden]){display:none}
:host([theme=dark]){
  --sp-feed-bg:#0f1222;--sp-feed-fg:#f7f8fb;--sp-feed-muted:#969cb0;--sp-feed-border:#363b4d;
  --sp-feed-hover:#1a1d2e;--sp-feed-accent:#a594ff;--sp-feed-accent-bg:#1b1150;
  --sp-feed-system:#67e3f4;--sp-feed-diagnostic:#fcd34d;--sp-feed-live:#4ade80;--sp-feed-error:#fca5a5;color-scheme:dark
}
@media (prefers-color-scheme:dark){:host(:not([theme=light]):not([theme=dark])){
  --sp-feed-bg:#0f1222;--sp-feed-fg:#f7f8fb;--sp-feed-muted:#969cb0;--sp-feed-border:#363b4d;
  --sp-feed-hover:#1a1d2e;--sp-feed-accent:#a594ff;--sp-feed-accent-bg:#1b1150;
  --sp-feed-system:#67e3f4;--sp-feed-diagnostic:#fcd34d;--sp-feed-live:#4ade80;--sp-feed-error:#fca5a5;color-scheme:dark
}}
.c{background:var(--sp-feed-bg);color:var(--sp-feed-fg);border:1px solid var(--sp-feed-border);
  border-radius:var(--sp-feed-radius);font:14px/1.5 var(--sp-feed-font);overflow:hidden}
header{display:flex;align-items:center;justify-content:space-between;gap:8px;padding:10px 14px;
  border-bottom:1px solid var(--sp-feed-border)}
h2{margin:0;font-size:14px;font-weight:600}
.live{display:inline-flex;align-items:center;gap:6px;font-size:12px;color:var(--sp-feed-muted)}
.live i{width:7px;height:7px;border-radius:50%;background:var(--sp-feed-muted)}
.live[data-s=open]{color:var(--sp-feed-live)}
.live[data-s=open] i{background:var(--sp-feed-live);animation:p 1.6s infinite}
@keyframes p{0%{box-shadow:0 0 0 0 var(--sp-feed-live)}70%,100%{box-shadow:0 0 0 5px transparent}}
ol{list-style:none;margin:0;padding:0;max-height:var(--sp-feed-max-height);overflow-y:auto}
li{display:grid;grid-template-columns:auto 1fr auto;gap:2px 10px;align-items:baseline;padding:10px 14px;
  border-top:1px solid var(--sp-feed-border)}
li:first-child{border-top:0}
li:hover{background:var(--sp-feed-hover)}
li.new{animation:n 1.2s ease-out}
@keyframes n{from{background:var(--sp-feed-accent-bg)}}
.t{font-size:11px;font-weight:500;padding:1px 6px;border-radius:4px;background:var(--sp-feed-accent-bg);color:var(--sp-feed-accent)}
.t.system{color:var(--sp-feed-system)}.t.diagnostic{color:var(--sp-feed-diagnostic)}
.s{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-weight:500}
time{font-size:12px;color:var(--sp-feed-muted);white-space:nowrap}
.m{grid-column:2/-1;font:12px var(--sp-feed-font-mono);color:var(--sp-feed-muted);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
a{color:var(--sp-feed-accent);text-decoration:none}a:hover{text-decoration:underline}
.e{margin:0;padding:24px 14px;text-align:center;color:var(--sp-feed-muted)}
.e.err{color:var(--sp-feed-error)}
footer{padding:6px 14px;border-top:1px solid var(--sp-feed-border);font-size:11px;color:var(--sp-feed-muted);text-align:right}
@media (prefers-reduced-motion:reduce){li.new,.live[data-s=open] i{animation:none}}
`;

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  cls?: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}

function topicLabel(t: unknown): string {
  if (typeof t === 'string') return t;
  if (t && typeof t === 'object') {
    const o = t as Record<string, unknown>;
    for (const k of ['symbol', 'sym', 'string', 'str', 'address']) {
      if (typeof o[k] === 'string') return o[k] as string;
    }
  }
  return '';
}

/** REST rows are snake_case; SSE frames are the camelCase SorobanEvent. */
export function normalizeEvent(raw: Record<string, any>): FeedEvent {
  const data = raw.event_data ?? raw.value;
  const topic = raw.topic ?? data?.topic ?? data?.topics;
  return {
    id: raw.id,
    contractId: raw.contract_id ?? raw.contractId ?? '',
    type: String(raw.event_type ?? raw.type ?? 'contract').toLowerCase(),
    txHash: raw.tx_hash ?? raw.txHash ?? '',
    ledger: Number(raw.ledger ?? 0),
    timestamp: raw.timestamp ?? raw.ledgerClosedAt ?? '',
    topics: Array.isArray(topic) ? topic.map(topicLabel).filter(Boolean) : [],
    raw,
  };
}

const short = (s: string) => (s.length > 14 ? `${s.slice(0, 6)}…${s.slice(-4)}` : s);

function ago(iso: string): string {
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return '';
  const s = Math.max(0, Math.round((Date.now() - t) / 1000));
  if (s < 60) return `${s}s ago`;
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  return `${Math.floor(s / 86400)}d ago`;
}

const key = (e: FeedEvent) => e.id ?? `${e.txHash}:${e.ledger}:${e.topics.join(',')}`;

export class SorobanPulseFeed extends HTMLElement {
  static observedAttributes = ['server', 'contract-id', 'limit', 'live', 'api-key', 'explorer-url', 'heading'];

  #root: ShadowRoot;
  #list = el('ol', '', undefined);
  #msg = el('p', 'e');
  #live = el('span', 'live');
  #heading = el('h2');
  #events: FeedEvent[] = [];
  #abort?: AbortController;
  #timer?: number;
  #queued = false;

  constructor() {
    super();
    this.#root = this.attachShadow({ mode: 'open' });
    const style = el('style', '', STYLES);
    const c = el('div', 'c');
    c.setAttribute('part', 'container');
    const header = el('header');
    header.setAttribute('part', 'header');
    this.#live.append(el('i'), el('span'));
    this.#live.hidden = true;
    header.append(this.#heading, this.#live);
    this.#list.setAttribute('part', 'list');
    this.#list.setAttribute('aria-live', 'polite');
    const footer = el('footer');
    footer.setAttribute('part', 'footer');
    const a = el('a', '', 'SorobanPulse');
    a.href = 'https://github.com/Soroban-Pulse/SorobanPulse';
    a.target = '_blank';
    a.rel = 'noopener';
    footer.append('Powered by ', a);
    c.append(header, this.#list, this.#msg, footer);
    this.#root.append(style, c);
  }

  get server(): string {
    return (this.getAttribute('server') ?? '').replace(/\/+$/, '');
  }
  get contractId(): string {
    return (this.getAttribute('contract-id') ?? '').trim();
  }
  get limit(): number {
    const n = parseInt(this.getAttribute('limit') ?? '', 10);
    return Number.isFinite(n) ? Math.min(Math.max(n, 1), MAX_LIMIT) : 10;
  }
  get live(): boolean {
    const v = this.getAttribute('live');
    return v !== null && v !== 'false';
  }
  /** Current events, newest first. */
  get events(): readonly FeedEvent[] {
    return this.#events;
  }

  connectedCallback(): void {
    this.#restart();
    this.#timer = window.setInterval(() => this.#renderList(), 30_000);
  }

  disconnectedCallback(): void {
    this.#abort?.abort();
    clearInterval(this.#timer);
  }

  attributeChangedCallback(name: string, oldV: string | null, newV: string | null): void {
    if (!this.isConnected || oldV === newV) return;
    if (name === 'heading' || name === 'explorer-url') return this.#renderList();
    // Batch changes made in the same task (e.g. setting several attributes).
    if (this.#queued) return;
    this.#queued = true;
    queueMicrotask(() => {
      this.#queued = false;
      this.#restart();
    });
  }

  /** Re-fetch the latest events (and reconnect the live stream if enabled). */
  refresh(): void {
    this.#restart();
  }

  #headers(accept: string): Record<string, string> {
    const h: Record<string, string> = { Accept: accept };
    const k = this.getAttribute('api-key');
    if (k) h['X-Api-Key'] = k;
    return h;
  }

  #base(): string {
    return `${this.server}/v1/events/contract/${encodeURIComponent(this.contractId)}`;
  }

  #restart(): void {
    this.#abort?.abort();
    const ctrl = (this.#abort = new AbortController());
    this.#events = [];
    this.#heading.textContent = this.getAttribute('heading') ?? 'Recent activity';
    if (!this.server || !this.contractId) {
      this.#setLive(null);
      return this.#message('Set the server and contract-id attributes.', true);
    }
    this.#message('Loading…');
    void this.#fetch(ctrl.signal).then(() => {
      if (this.live && !ctrl.signal.aborted) void this.#stream(ctrl.signal);
    });
    this.#setLive(this.live ? 'connecting' : null);
  }

  async #fetch(signal: AbortSignal): Promise<void> {
    try {
      const res = await fetch(`${this.#base()}?limit=${this.limit}&sort=desc`, {
        headers: this.#headers('application/json'),
        signal,
      });
      if (res.status === 404) {
        this.#events = [];
      } else if (!res.ok) {
        return this.#fail(res.status);
      } else {
        const body = await res.json();
        this.#events = (Array.isArray(body) ? body : body.data ?? []).map(normalizeEvent);
      }
      this.#renderList();
    } catch (err) {
      if (!signal.aborted) this.#fail(0, err);
    }
  }

  async #stream(signal: AbortSignal): Promise<void> {
    let delay = 1000;
    let lastId = '';
    while (!signal.aborted) {
      this.#setLive('connecting');
      try {
        const h = this.#headers('text/event-stream');
        if (lastId) h['Last-Event-ID'] = lastId;
        const res = await fetch(`${this.#base()}/stream`, { headers: h, signal });
        if (!res.ok || !res.body) {
          if (res.status === 401 || res.status === 403) return this.#fail(res.status);
          throw new Error(String(res.status));
        }
        this.#setLive('open');
        delay = 1000;
        const reader = res.body.pipeThrough(new TextDecoderStream()).getReader();
        let buf = '';
        for (;;) {
          const { value, done } = await reader.read();
          if (done) break;
          buf += value;
          const frames = buf.split(/\r?\n\r?\n/);
          buf = frames.pop() ?? '';
          for (const f of frames) {
            let data = '';
            let event = '';
            for (const line of f.split(/\r?\n/)) {
              if (line.startsWith('data:')) data += (data ? '\n' : '') + line.slice(5).replace(/^ /, '');
              else if (line.startsWith('id:')) lastId = line.slice(3).trim();
              else if (line.startsWith('event:')) event = line.slice(6).trim();
            }
            if (data && (!event || event === 'message')) this.#push(data);
          }
        }
      } catch {
        if (signal.aborted) return;
      }
      this.#setLive('error');
      await new Promise((r) => setTimeout(r, delay));
      delay = Math.min(delay * 2, 30_000);
    }
  }

  #push(data: string): void {
    let ev: FeedEvent;
    try {
      ev = normalizeEvent(JSON.parse(data));
    } catch {
      return;
    }
    if (ev.contractId && ev.contractId !== this.contractId) return;
    const k = key(ev);
    if (this.#events.some((e) => key(e) === k)) return;
    this.#events = [ev, ...this.#events].slice(0, this.limit);
    this.#renderList(k);
    this.dispatchEvent(new CustomEvent('sp-feed-event', { detail: ev, bubbles: true, composed: true }));
  }

  #fail(status: number, err?: unknown): void {
    const message =
      status === 401 || status === 403
        ? 'This feed needs a valid API key.'
        : status === 429
          ? 'Rate limited — try again shortly.'
          : status
            ? `Could not load events (${status}).`
            : 'Could not reach the SorobanPulse server.';
    this.#setLive(null);
    this.#message(message, true);
    this.dispatchEvent(
      new CustomEvent('sp-feed-error', { detail: { message, status, error: err }, bubbles: true, composed: true }),
    );
  }

  #setLive(s: 'connecting' | 'open' | 'error' | null): void {
    this.#live.hidden = s === null;
    if (s) {
      this.#live.dataset.s = s;
      this.#live.lastElementChild!.textContent = s === 'open' ? 'Live' : 'Connecting…';
    }
  }

  #message(text: string, error = false): void {
    this.#list.replaceChildren();
    this.#msg.textContent = text;
    this.#msg.className = error ? 'e err' : 'e';
    this.#msg.hidden = false;
  }

  #renderList(newKey?: string): void {
    this.#heading.textContent = this.getAttribute('heading') ?? 'Recent activity';
    if (!this.#events.length) {
      if (this.#msg.className === 'e err') return;
      return this.#message('No events yet.');
    }
    this.#msg.hidden = true;
    const explorer = this.getAttribute('explorer-url');
    this.#list.replaceChildren(
      ...this.#events.map((ev) => {
        const li = el('li', newKey && key(ev) === newKey ? 'new' : '');
        li.setAttribute('part', 'item');
        const t = el('span', `t ${ev.type}`, ev.type);
        const summary = el('span', 's', ev.topics.slice(0, 2).join(' · ') || `${ev.type} event`);
        const time = el('time', '', ago(ev.timestamp));
        if (ev.timestamp) time.dateTime = ev.timestamp;
        time.title = ev.timestamp;
        const meta = el('div', 'm', `Ledger ${ev.ledger.toLocaleString()} · `);
        if (explorer && ev.txHash && /^https?:\/\//.test(explorer)) {
          const a = el('a', '', short(ev.txHash));
          a.href = explorer.replace('{tx}', encodeURIComponent(ev.txHash));
          a.target = '_blank';
          a.rel = 'noopener';
          meta.append(a);
        } else {
          meta.append(short(ev.txHash));
        }
        li.append(t, summary, time, meta);
        return li;
      }),
    );
  }
}

if (!customElements.get('soroban-pulse-feed')) {
  customElements.define('soroban-pulse-feed', SorobanPulseFeed);
}

declare global {
  interface HTMLElementTagNameMap {
    'soroban-pulse-feed': SorobanPulseFeed;
  }
}
