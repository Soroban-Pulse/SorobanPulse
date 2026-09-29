import { getSettings } from "./settings";

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
    public body?: unknown,
  ) {
    super(message);
  }
}

export type Auth = "none" | "user" | "admin";

export interface RequestOptions {
  method?: string;
  body?: unknown;
  /** Which key to send. "user" falls back to the admin key when no user key is set. */
  auth?: Auth;
  signal?: AbortSignal;
  timeoutMs?: number;
}

export interface RawResponse<T> {
  status: number;
  ok: boolean;
  data: T | undefined;
}

function authHeader(auth: Auth): Record<string, string> {
  const { apiKey, adminKey } = getSettings();
  const key = auth === "admin" ? adminKey : auth === "user" ? apiKey || adminKey : "";
  return key ? { Authorization: `Bearer ${key}` } : {};
}

export function apiUrl(path: string): string {
  return getSettings().apiBaseUrl.replace(/\/+$/, "") + path;
}

/**
 * Low-level request that resolves for any HTTP status (health endpoints use
 * 503 to carry a meaningful body). Rejects only on network errors/timeouts.
 */
export async function requestRaw<T = unknown>(path: string, opts: RequestOptions = {}): Promise<RawResponse<T>> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), opts.timeoutMs ?? 10_000);
  opts.signal?.addEventListener("abort", () => controller.abort());
  try {
    const res = await fetch(apiUrl(path), {
      method: opts.method ?? "GET",
      headers: {
        Accept: "application/json",
        ...(opts.body !== undefined ? { "Content-Type": "application/json" } : {}),
        ...authHeader(opts.auth ?? "user"),
      },
      body: opts.body !== undefined ? JSON.stringify(opts.body) : undefined,
      signal: controller.signal,
    });
    const text = await res.text();
    let data: T | undefined;
    try {
      data = text ? (JSON.parse(text) as T) : undefined;
    } catch {
      data = undefined;
    }
    return { status: res.status, ok: res.ok, data };
  } finally {
    clearTimeout(timer);
  }
}

/** Request that rejects with ApiError on non-2xx responses. */
export async function api<T = unknown>(path: string, opts: RequestOptions = {}): Promise<T> {
  const res = await requestRaw<T>(path, opts);
  if (!res.ok) {
    const body = res.data as { error?: string; message?: string } | undefined;
    const message = body?.error ?? body?.message ?? `Request failed with status ${res.status}`;
    throw new ApiError(res.status, message, res.data);
  }
  return res.data as T;
}

export function errorMessage(err: unknown): string {
  if (err instanceof ApiError) {
    if (err.status === 401) return "Unauthorized — check the API key in Settings.";
    if (err.status === 403) return `Forbidden — ${err.message}`;
    return err.message;
  }
  if (err instanceof DOMException && err.name === "AbortError") return "Request timed out.";
  if (err instanceof Error) return err.message;
  return String(err);
}

export function qs(params: Record<string, string | number | boolean | undefined | null>): string {
  const search = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined && v !== null && v !== "") search.set(k, String(v));
  }
  const s = search.toString();
  return s ? `?${s}` : "";
}
