// Client-side mirror of `validate_callback_url` in src/subscriptions.rs.
// The server stays authoritative (it also applies SUBSCRIPTION_ALLOWED_URL_PREFIXES,
// which the browser cannot see); this just catches mistakes before a round trip.

export interface UrlRuleOptions {
  /** Production and staging only accept https. */
  requireHttps: boolean;
}

function isPrivateIpv4(host: string): boolean {
  const parts = host.split('.');
  if (parts.length !== 4 || parts.some((p) => !/^\d{1,3}$/.test(p))) return false;
  const [a, b, c] = parts.map(Number);
  return (
    a === 127 ||
    a === 10 ||
    (a === 172 && b >= 16 && b <= 31) ||
    (a === 192 && b === 168) ||
    (a === 169 && b === 254) ||
    (a === 0 && b === 0 && c === 0) ||
    a >= 224
  );
}

function isPrivateIpv6(host: string): boolean {
  const h = host.replace(/^\[|\]$/g, '').toLowerCase();
  if (!h.includes(':')) return false;
  if (h === '::1' || h === '::') return true;
  const first = parseInt(h.split(':')[0] || '0', 16);
  return (first & 0xffc0) === 0xfe80 || (first & 0xfe00) === 0xfc00;
}

export function isSsrfHost(host: string): boolean {
  const h = host.toLowerCase();
  if (h === 'localhost' || h.endsWith('.local') || h.endsWith('.localhost')) return true;
  if (isPrivateIpv4(h) || isPrivateIpv6(h)) return true;
  if (h.startsWith('10.') || h.startsWith('192.168.') || h.startsWith('169.254.')) return true;
  if (h.startsWith('172.')) {
    const second = Number(h.split('.')[1]);
    if (second >= 16 && second <= 31) return true;
  }
  return false;
}

/** Returns an error message, or null when the URL passes. */
export function validateCallbackUrl(raw: string, opts: UrlRuleOptions): string | null {
  if (!raw.trim()) return 'callback_url is required';
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    return 'callback_url is not a valid URL';
  }
  if (url.protocol === 'http:') {
    if (opts.requireHttps) return 'callback_url must use HTTPS in production';
  } else if (url.protocol !== 'https:') {
    return `callback_url scheme '${url.protocol.replace(':', '')}' is not permitted; use https`;
  }
  if (isSsrfHost(url.hostname)) {
    return 'callback_url points to a private, loopback, or link-local address';
  }
  return null;
}

const CONTRACT_ID_RE = /^C[A-Z2-7]{55}$/;

export function isContractId(s: string): boolean {
  return CONTRACT_ID_RE.test(s);
}
