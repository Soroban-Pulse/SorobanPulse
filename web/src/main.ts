import '../../design/build/tokens.css';
import '../../design/components.css';
import './styles/app.css';

import { ApiError, PulseClient, PulseEvent, loadSettings, saveSettings } from './api';
import { eventBadge, h, jsonBlock, relativeTime, shortHash } from './format';

type Theme = 'auto' | 'light' | 'dark';
type LiveState = 'off' | 'connecting' | 'open' | 'error';

const THEME_KEY = 'sp.dashboard.theme';
const MAX_ROWS = 500;

const state = {
  settings: loadSettings(),
  tab: 'events' as 'events' | 'settings',
  contractId: new URLSearchParams(location.search).get('contract') ?? '',
  eventType: '',
  limit: 50,
  page: 1,
  events: [] as PulseEvent[],
  loading: false,
  error: '',
  live: 'off' as LiveState,
  selected: null as PulseEvent | null,
};

let client = new PulseClient(state.settings);
let stopStream: (() => void) | null = null;

// ---------- Theme ----------

function readTheme(): Theme {
  try {
    const t = localStorage.getItem(THEME_KEY);
    return t === 'light' || t === 'dark' ? t : 'auto';
  } catch {
    return 'auto';
  }
}

function applyTheme(t: Theme): void {
  if (t === 'auto') document.documentElement.removeAttribute('data-theme');
  else document.documentElement.setAttribute('data-theme', t);
  try {
    localStorage.setItem(THEME_KEY, t);
  } catch {
    /* ignore */
  }
}

let theme = readTheme();
applyTheme(theme);

// ---------- Toasts ----------

function toast(message: string, kind: 'info' | 'success' | 'warn' | 'error' = 'info'): void {
  const region = document.getElementById('toasts')!;
  const el = h(
    'div',
    { class: `sp-toast sp-toast--${kind}`, role: kind === 'error' ? 'alert' : undefined },
    h('div', { class: 'sp-toast__body' }, message),
    h('button', { class: 'sp-btn sp-btn--ghost sp-btn--sm', 'aria-label': 'Dismiss', onclick: () => el.remove() }, '✕'),
  );
  region.append(el);
  setTimeout(() => el.remove(), kind === 'error' ? 8000 : 4000);
}

// ---------- Data ----------

async function load(): Promise<void> {
  state.loading = true;
  state.error = '';
  render();
  try {
    state.events = await client.events({
      contractId: state.contractId.trim() || undefined,
      eventType: state.eventType || undefined,
      page: state.page,
      limit: state.limit,
    });
  } catch (err) {
    state.events = [];
    state.error =
      err instanceof ApiError && err.status === 401
        ? 'The server requires an API key. Add one in Settings.'
        : err instanceof Error
          ? err.message
          : String(err);
  } finally {
    state.loading = false;
    render();
  }
}

function setLive(on: boolean): void {
  stopStream?.();
  stopStream = null;
  if (!on) {
    state.live = 'off';
    render();
    return;
  }
  stopStream = client.stream(
    { contractId: state.contractId.trim() || undefined, eventType: state.eventType || undefined },
    (ev) => {
      if (state.page !== 1) return;
      state.events = [ev, ...state.events].slice(0, MAX_ROWS);
      renderTable();
    },
    (s) => {
      const was = state.live;
      state.live = s;
      if (s === 'error' && was === 'open') toast('Live stream disconnected — reconnecting…', 'warn');
      renderHeader();
      renderFilters();
    },
  );
}

// ---------- Views ----------

const app = document.getElementById('app')!;
const headerSlot = h('div');
const mainSlot = h('main', { class: 'app-main' });
const drawerSlot = h('div');
app.append(headerSlot, mainSlot, drawerSlot);

let filtersSlot: HTMLElement | null = null;
let tableSlot: HTMLElement | null = null;

function logo(): SVGSVGElement {
  const ns = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(ns, 'svg');
  svg.setAttribute('viewBox', '0 0 64 64');
  svg.setAttribute('class', 'app-logo');
  svg.setAttribute('aria-hidden', 'true');
  const make = (tag: string, attrs: Record<string, string>) => {
    const el = document.createElementNS(ns, tag);
    for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
    svg.append(el);
    return el;
  };
  make('rect', { x: '2', y: '2', width: '60', height: '60', rx: '15', class: 'app-logo__bg' });
  make('ellipse', {
    cx: '32', cy: '32', rx: '25', ry: '10.5', transform: 'rotate(-24 32 32)',
    fill: 'none', stroke: '#fff', 'stroke-opacity': '0.32', 'stroke-width': '2',
  });
  make('polyline', {
    points: '9 34 20 34 25.5 21 32.5 46 38.5 27 42.5 34 55 34', fill: 'none', stroke: '#fff',
    'stroke-width': '4.5', 'stroke-linecap': 'round', 'stroke-linejoin': 'round',
  });
  make('circle', { cx: '51.5', cy: '16.5', r: '3.2', fill: '#fff' });
  return svg;
}

function renderHeader(): void {
  const liveLabel: Record<LiveState, string> = {
    off: 'Paused', connecting: 'Connecting…', open: 'Live', error: 'Reconnecting…',
  };
  const liveClass: Record<LiveState, string> = {
    off: '', connecting: 'sp-badge--warn', open: 'sp-badge--success', error: 'sp-badge--error',
  };
  const tab = (id: typeof state.tab, label: string) =>
    h('button', {
      class: 'sp-tabs__tab', role: 'tab', 'aria-selected': String(state.tab === id),
      onclick: () => { state.tab = id; render(); },
    }, label);

  headerSlot.replaceChildren(
    h('header', { class: 'app-header' },
      h('div', { class: 'app-header__inner' },
        h('a', { class: 'app-brand', href: './' }, logo(),
          h('span', { class: 'app-brand__word' }, 'Soroban', h('b', {}, 'Pulse'))),
        h('nav', { class: 'sp-tabs app-tabs', role: 'tablist', 'aria-label': 'Sections' },
          tab('events', 'Events'), tab('settings', 'Settings')),
        h('div', { class: 'app-header__end' },
          h('span', { class: `sp-badge ${liveClass[state.live]}` },
            h('span', { class: `sp-dot ${state.live === 'open' ? 'sp-dot--live' : ''}` }),
            liveLabel[state.live]),
          h('select', {
            class: 'sp-select app-theme', 'aria-label': 'Theme',
            onchange: (e: Event) => { theme = (e.target as HTMLSelectElement).value as Theme; applyTheme(theme); },
          },
            ...(['auto', 'light', 'dark'] as Theme[]).map((t) =>
              h('option', { value: t, selected: theme === t }, t === 'auto' ? 'System theme' : t === 'light' ? 'Light' : 'Dark'))),
        ),
      ),
    ),
  );
}

function renderFilters(): void {
  if (!filtersSlot) return;
  const onEnter = (e: KeyboardEvent) => { if (e.key === 'Enter') apply(); };
  const apply = () => {
    state.page = 1;
    void load();
    if (state.live !== 'off') setLive(true);
  };
  filtersSlot.replaceChildren(
    h('div', { class: 'app-filters' },
      h('label', { class: 'sp-field app-filters__contract' },
        h('span', { class: 'sp-field__label' }, 'Contract ID'),
        h('input', {
          class: 'sp-input sp-mono', placeholder: 'C… (leave empty for all contracts)', value: state.contractId,
          spellcheck: false, autocomplete: 'off',
          oninput: (e: Event) => { state.contractId = (e.target as HTMLInputElement).value; },
          onkeydown: onEnter,
        })),
      h('label', { class: 'sp-field' },
        h('span', { class: 'sp-field__label' }, 'Type'),
        h('select', {
          class: 'sp-select',
          onchange: (e: Event) => { state.eventType = (e.target as HTMLSelectElement).value; apply(); },
        },
          ...[['', 'All types'], ['contract', 'Contract'], ['system', 'System'], ['diagnostic', 'Diagnostic']].map(
            ([v, l]) => h('option', { value: v, selected: state.eventType === v }, l)))),
      h('label', { class: 'sp-field' },
        h('span', { class: 'sp-field__label' }, 'Rows'),
        h('select', {
          class: 'sp-select',
          onchange: (e: Event) => { state.limit = Number((e.target as HTMLSelectElement).value); apply(); },
        }, ...[25, 50, 100].map((n) => h('option', { value: String(n), selected: state.limit === n }, String(n))))),
      h('div', { class: 'app-filters__actions' },
        h('button', { class: 'sp-btn sp-btn--primary', onclick: apply, disabled: state.loading }, state.loading ? 'Loading…' : 'Apply'),
        h('button', {
          class: 'sp-btn sp-btn--secondary', 'aria-pressed': String(state.live !== 'off'),
          onclick: () => setLive(state.live === 'off'),
        }, h('span', { class: 'sp-dot' }), state.live === 'off' ? 'Go live' : 'Stop live')),
    ),
  );
}

function renderTable(): void {
  if (!tableSlot) return;
  const rows = state.events.map((ev) =>
    h('tr', {
      tabindex: 0, 'aria-selected': String(state.selected === ev),
      onclick: () => openDrawer(ev),
      onkeydown: (e: KeyboardEvent) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); openDrawer(ev); } },
    },
      h('td', { class: 'sp-table__num' }, ev.ledger ? ev.ledger.toLocaleString() : '—'),
      h('td', { title: ev.timestamp }, relativeTime(ev.timestamp)),
      h('td', {}, eventBadge(ev.type)),
      h('td', { class: 'sp-mono', title: ev.contractId }, shortHash(ev.contractId, 8, 6)),
      h('td', { class: 'sp-mono', title: ev.txHash }, shortHash(ev.txHash, 8, 6)),
    ));

  const empty = state.loading
    ? 'Loading events…'
    : state.error
      ? state.error
      : state.contractId
        ? 'No events for this contract yet.'
        : 'No events indexed yet.';

  tableSlot.replaceChildren(
    h('div', { class: 'sp-table-wrap' },
      h('table', { class: 'sp-table' },
        h('thead', {}, h('tr', {}, ...['Ledger', 'Time', 'Type', 'Contract', 'Transaction'].map((c) => h('th', { scope: 'col' }, c)))),
        h('tbody', {}, ...(rows.length
          ? rows
          : [h('tr', {}, h('td', { colspan: 5, class: `sp-table__empty ${state.error ? 'app-error' : ''}` }, empty))])),
      ),
    ),
    h('div', { class: 'app-pager' },
      h('button', {
        class: 'sp-btn sp-btn--secondary sp-btn--sm', disabled: state.page <= 1 || state.loading,
        onclick: () => { state.page--; void load(); },
      }, '← Newer'),
      h('span', { class: 'app-pager__label' }, `Page ${state.page}`),
      h('button', {
        class: 'sp-btn sp-btn--secondary sp-btn--sm', disabled: state.events.length < state.limit || state.loading,
        onclick: () => { state.page++; void load(); },
      }, 'Older →'),
    ),
  );
}

function renderSettings(): HTMLElement {
  let server = state.settings.server;
  let apiKey = state.settings.apiKey;
  return h('form', {
    class: 'sp-card app-settings',
    onsubmit: (e: Event) => {
      e.preventDefault();
      state.settings = { server: server.trim(), apiKey: apiKey.trim() };
      saveSettings(state.settings);
      client = new PulseClient(state.settings);
      if (state.live !== 'off') setLive(true);
      toast('Settings saved', 'success');
      void load();
    },
  },
    h('h2', { class: 'app-settings__title' }, 'Connection'),
    h('label', { class: 'sp-field' },
      h('span', { class: 'sp-field__label' }, 'API server'),
      h('input', {
        class: 'sp-input', type: 'url', placeholder: location.origin, value: server,
        oninput: (e: Event) => { server = (e.target as HTMLInputElement).value; },
      }),
      h('span', { class: 'sp-field__hint' }, 'Leave empty to use the server hosting this dashboard.')),
    h('label', { class: 'sp-field' },
      h('span', { class: 'sp-field__label' }, 'API key'),
      h('input', {
        class: 'sp-input sp-mono', type: 'password', autocomplete: 'off', value: apiKey,
        oninput: (e: Event) => { apiKey = (e.target as HTMLInputElement).value; },
      }),
      h('span', { class: 'sp-field__hint' }, 'Sent as the X-Api-Key header. Stored in this browser only.')),
    h('div', {}, h('button', { class: 'sp-btn sp-btn--primary', type: 'submit' }, 'Save')),
  );
}

function openDrawer(ev: PulseEvent | null): void {
  state.selected = ev;
  renderTable();
  if (!ev) {
    drawerSlot.replaceChildren();
    return;
  }
  const close = () => openDrawer(null);
  const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') { document.removeEventListener('keydown', onKey); close(); } };
  document.addEventListener('keydown', onKey);
  const dl = (label: string, value: Node | string) =>
    [h('dt', {}, label), h('dd', {}, value)];
  const closeBtn = h('button', { class: 'sp-btn sp-btn--ghost sp-btn--icon', 'aria-label': 'Close', onclick: close }, '✕');
  drawerSlot.replaceChildren(
    h('div', { class: 'sp-drawer-backdrop', onclick: close }),
    h('aside', { class: 'sp-drawer', role: 'dialog', 'aria-modal': 'true', 'aria-label': 'Event details' },
      h('div', { class: 'sp-drawer__header' }, h('h2', { class: 'sp-drawer__title' }, 'Event'), closeBtn),
      h('div', { class: 'sp-drawer__body' },
        h('dl', { class: 'app-dl' },
          ...dl('Type', eventBadge(ev.type)),
          ...dl('Ledger', ev.ledger ? ev.ledger.toLocaleString() : '—'),
          ...dl('Time', ev.timestamp || '—'),
          ...dl('Contract', h('span', { class: 'sp-mono app-break' }, ev.contractId)),
          ...dl('Transaction', h('span', { class: 'sp-mono app-break' }, ev.txHash))),
        h('div', { class: 'app-drawer-actions' },
          h('button', {
            class: 'sp-btn sp-btn--secondary sp-btn--sm',
            onclick: () => { state.contractId = ev.contractId; state.page = 1; close(); render(); void load(); },
          }, 'Filter by this contract'),
          h('button', {
            class: 'sp-btn sp-btn--secondary sp-btn--sm',
            onclick: async () => {
              try {
                await navigator.clipboard.writeText(JSON.stringify(ev.raw, null, 2));
                toast('Copied event JSON', 'success');
              } catch {
                toast('Clipboard is not available here', 'warn');
              }
            },
          }, 'Copy JSON')),
        jsonBlock(ev.raw)),
    ),
  );
  closeBtn.focus();
}

function render(): void {
  renderHeader();
  if (state.tab === 'settings') {
    filtersSlot = tableSlot = null;
    mainSlot.replaceChildren(renderSettings());
    return;
  }
  filtersSlot = h('section', { 'aria-label': 'Filters' });
  tableSlot = h('section', { 'aria-label': 'Events', 'aria-busy': String(state.loading) });
  mainSlot.replaceChildren(filtersSlot, tableSlot);
  renderFilters();
  renderTable();
}

// Keep relative timestamps fresh.
setInterval(() => { if (state.tab === 'events' && !state.selected) renderTable(); }, 30_000);

render();
void load();
