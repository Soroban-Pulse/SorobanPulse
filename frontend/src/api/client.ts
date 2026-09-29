// Thin fetch wrapper for the Soroban Pulse REST API.
//
// Credentials are kept in localStorage so the UI can talk to a deployment that
// requires API_KEY (regular routes) and/or ADMIN_API_KEY (/admin routes).

const STORAGE_KEY = 'soroban-pulse.settings';

export interface ApiSettings {
  baseUrl: string;
  apiKey: string;
  adminApiKey: string;
  /** Mirror of the server's production-like environment check for callback URLs. */
  requireHttpsCallbacks: boolean;
}

const DEFAULTS: ApiSettings = { baseUrl: '', apiKey: '', adminApiKey: '', requireHttpsCallbacks: true };

export function loadSettings(): ApiSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) return { ...DEFAULTS, ...JSON.parse(raw) };
  } catch {
    // Storage unavailable (private mode); fall through to defaults.
  }
  return { ...DEFAULTS };
}

export function saveSettings(settings: ApiSettings): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch {
    // Ignore; settings simply won't persist.
  }
}

/** Error raised for any non-2xx response. `fieldErrors` maps form fields to messages when the server names them. */
export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
    public body: unknown,
    public fieldErrors: Record<string, string> = {},
  ) {
    super(message);
  }
}

// The server returns `{ "error": "callback_url is not a valid URL: ..." }` for
// validation failures. Pull the leading snake_case identifier out as the field
// name so forms can render the message next to the right input.
function extractFieldErrors(message: string, body: unknown): Record<string, string> {
  const out: Record<string, string> = {};
  if (body && typeof body === 'object' && 'details' in body) {
    const details = (body as { details: unknown }).details;
    if (details && typeof details === 'object') {
      for (const [k, v] of Object.entries(details)) out[k] = String(v);
    }
  }
  const m = /^([a-z][a-z0-9_]*) /.exec(message);
  if (m && !out[m[1]]) out[m[1]] = message;
  return out;
}

export interface RequestOptions {
  method?: string;
  body?: unknown;
  query?: Record<string, string | number | boolean | undefined | null>;
  admin?: boolean;
  signal?: AbortSignal;
}

export async function request<T>(path: string, opts: RequestOptions = {}): Promise<T> {
  const settings = loadSettings();
  const url = new URL(settings.baseUrl.replace(/\/$/, '') + path, window.location.origin);
  for (const [k, v] of Object.entries(opts.query ?? {})) {
    if (v !== undefined && v !== null && v !== '') url.searchParams.set(k, String(v));
  }

  const headers: Record<string, string> = { Accept: 'application/json' };
  const key = opts.admin ? settings.adminApiKey || settings.apiKey : settings.apiKey;
  if (key) headers.Authorization = `Bearer ${key}`;
  if (opts.body !== undefined) headers['Content-Type'] = 'application/json';

  const res = await fetch(url, {
    method: opts.method ?? 'GET',
    headers,
    body: opts.body !== undefined ? JSON.stringify(opts.body) : undefined,
    signal: opts.signal,
  });

  const text = await res.text();
  let body: unknown = undefined;
  if (text) {
    try {
      body = JSON.parse(text);
    } catch {
      body = text;
    }
  }

  if (!res.ok) {
    let message = `${res.status} ${res.statusText}`;
    if (body && typeof body === 'object' && 'error' in body) message = String((body as { error: unknown }).error);
    else if (typeof body === 'string' && body) message = body;
    throw new ApiError(res.status, message, body, extractFieldErrors(message, body));
  }
  return body as T;
}

export function errorMessage(err: unknown): string {
  if (err instanceof ApiError) {
    if (err.status === 401 || err.status === 403) {
      return `${err.message} — check the API keys in Settings.`;
    }
    if (err.status === 404) return `${err.message} (endpoint or resource not found)`;
    return err.message;
  }
  if (err instanceof Error) return err.message;
  return String(err);
}
