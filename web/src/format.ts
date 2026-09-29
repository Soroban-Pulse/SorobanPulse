// Small DOM and formatting helpers shared by the dashboard views.

type Child = Node | string | null | undefined | false;

/** Minimal hyperscript: h('div', { class: 'x', onclick: fn }, child, ...). */
export function h<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Record<string, unknown> = {},
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (v === undefined || v === null || v === false) continue;
    if (k.startsWith('on') && typeof v === 'function') {
      el.addEventListener(k.slice(2), v as EventListener);
    } else if (k === 'class') {
      el.className = String(v);
    } else if (k in el && typeof v !== 'string') {
      (el as any)[k] = v;
    } else {
      el.setAttribute(k, v === true ? '' : String(v));
    }
  }
  for (const c of children) {
    if (c !== null && c !== undefined && c !== false) el.append(c);
  }
  return el;
}

export function shortHash(s: string, head = 6, tail = 4): string {
  return s.length > head + tail + 1 ? `${s.slice(0, head)}…${s.slice(-tail)}` : s;
}

const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });

export function relativeTime(iso: string): string {
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return '—';
  const secs = Math.round((t - Date.now()) / 1000);
  const abs = Math.abs(secs);
  if (abs < 60) return rtf.format(secs, 'second');
  if (abs < 3600) return rtf.format(Math.round(secs / 60), 'minute');
  if (abs < 86400) return rtf.format(Math.round(secs / 3600), 'hour');
  return rtf.format(Math.round(secs / 86400), 'day');
}

/** Render JSON with token spans for the .sp-code block — no innerHTML involved. */
export function jsonBlock(value: unknown): HTMLPreElement {
  const pre = h('pre', { class: 'sp-code', tabindex: 0 });
  const text = JSON.stringify(value, null, 2) ?? 'null';
  const re = /("(?:\\.|[^"\\])*")(\s*:)?|\b(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)\b|\b(true|false|null)\b/g;
  let last = 0;
  for (let m: RegExpExecArray | null; (m = re.exec(text)); ) {
    pre.append(text.slice(last, m.index));
    const [, str, colon, num, lit] = m;
    if (str) {
      pre.append(h('span', { class: colon ? 'tok-key' : 'tok-string' }, str));
      if (colon) pre.append(colon);
    } else if (num) {
      pre.append(h('span', { class: 'tok-number' }, num));
    } else if (lit) {
      pre.append(h('span', { class: 'tok-literal' }, lit));
    }
    last = re.lastIndex;
  }
  pre.append(text.slice(last));
  return pre;
}

export function eventBadge(type: string): HTMLSpanElement {
  const known = ['contract', 'system', 'diagnostic'].includes(type);
  return h('span', { class: `sp-badge ${known ? `sp-badge--${type}` : ''}` }, type);
}
