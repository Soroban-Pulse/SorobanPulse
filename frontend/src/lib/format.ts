export function shortId(id: string, head = 6, tail = 6): string {
  return id.length <= head + tail + 1 ? id : `${id.slice(0, head)}…${id.slice(-tail)}`;
}

export function formatNumber(n: number | null | undefined): string {
  return n === null || n === undefined || Number.isNaN(n) ? "—" : n.toLocaleString();
}

export function formatDateTime(iso: string | null | undefined): string {
  if (!iso) return "—";
export function formatNumber(n: number | null | undefined): string {
  if (n === null || n === undefined || Number.isNaN(n)) return '—';
  return new Intl.NumberFormat().format(n);
}

export function formatCompact(n: number): string {
  return new Intl.NumberFormat(undefined, { notation: 'compact', maximumFractionDigits: 1 }).format(n);
}

export function formatDate(iso: string | null | undefined): string {
  if (!iso) return '—';
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString();
}

export function formatDuration(secs: number): string {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (d) return `${d}d ${h}h`;
  if (h) return `${h}h ${m}m`;
  if (m) return `${m}m`;
  return `${Math.floor(secs)}s`;
}

export function formatPercent(ratio: number, digits = 2): string {
  return Number.isFinite(ratio) ? `${(ratio * 100).toFixed(digits)}%` : "—";
export function formatRelative(iso: string | null | undefined): string {
  if (!iso) return '—';
  const diff = Date.now() - new Date(iso).getTime();
  const abs = Math.abs(diff);
  const units: [number, Intl.RelativeTimeFormatUnit][] = [
    [86_400_000, 'day'],
    [3_600_000, 'hour'],
    [60_000, 'minute'],
    [1000, 'second'],
  ];
  const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });
  for (const [ms, unit] of units) {
    if (abs >= ms || unit === 'second') return rtf.format(-Math.round(diff / ms), unit);
  }
  return iso;
}

/** Shorten long identifiers (contract IDs, hashes) to `CABC…WXYZ`. */
export function truncateMiddle(s: string, keep = 6): string {
  return s.length <= keep * 2 + 1 ? s : `${s.slice(0, keep)}…${s.slice(-keep)}`;
}

/** Hide credentials, path secrets and query tokens in a URL for list views. */
export function maskUrl(raw: string): string {
  try {
    const u = new URL(raw);
    const path = u.pathname.length > 12 ? `${u.pathname.slice(0, 8)}••••` : u.pathname;
    return `${u.protocol}//${u.host}${path}${u.search ? '?••••' : ''}`;
  } catch {
    return raw.length > 16 ? `${raw.slice(0, 12)}••••` : raw;
  }
}

/** Generate a 32-byte hex webhook signing secret in the browser. */
export function generateSecret(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return 'whsec_' + Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
}

export function parseList(raw: string): string[] {
  return raw
    .split(/[\s,]+/)
    .map((s) => s.trim())
    .filter(Boolean);
}
