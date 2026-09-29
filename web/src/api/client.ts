/**
 * Typed openapi-fetch client for the SorobanPulse API.
 *
 * Responsibilities:
 *  - Resolve the base URL from SettingsContext storage (sessionStorage → localStorage)
 *    so the user's server URL is always used at call-time.
 *  - Inject X-Api-Key and, when present, X-Admin-Key auth headers on every request.
 *  - Parse RFC 9457 Problem Detail responses into a typed ApiError.
 *  - Re-export the fully-typed path helpers so call sites get inference.
 *
 * Credentials are read directly from Web Storage at request-time so this
 * module stays free of React dependencies and works in any context (hooks,
 * event handlers, the settings page's "Test connection" fetch, etc.).
 */

import createClient, { type Middleware } from "openapi-fetch";
import type { paths, components } from "./schema.d.ts";

// ─── Storage key constants (must match SettingsContext.tsx) ───────────────────

const SK = {
  SERVER_URL: "sp_server_url",
  API_KEY: "sp_api_key",
  ADMIN_KEY: "sp_admin_key",
} as const;

// ─── Storage reader ───────────────────────────────────────────────────────────

/**
 * Reads a credential from sessionStorage first, then falls back to
 * localStorage (set when "remember on this device" is enabled).
 */
function readCredential(key: string): string {
  try {
    return (
      sessionStorage.getItem(key) ?? localStorage.getItem(key) ?? ""
    );
  } catch {
    // Storage may be unavailable in sandboxed contexts / tests.
    return "";
  }
}

// ─── RFC 9457 typed error ─────────────────────────────────────────────────────

export type ProblemDetail = components["schemas"]["ProblemDetail"];

export class ApiError extends Error {
  /** HTTP status code */
  readonly status: number;
  /** Full RFC 9457 problem detail (if the server sent one) */
  readonly problem: ProblemDetail | null;

  constructor(status: number, problem: ProblemDetail | null, message: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.problem = problem;
  }

  /** True when the error is an auth failure */
  get isUnauthorized() {
    return this.status === 401 || this.status === 403;
  }

  /** True when the resource was not found */
  get isNotFound() {
    return this.status === 404;
  }

  /** True when the client hit the rate limit */
  get isRateLimited() {
    return this.status === 429;
  }
}

// ─── Auth middleware ──────────────────────────────────────────────────────────

/**
 * Reads credentials from Web Storage at request-time so any change the user
 * saves on the Settings page is picked up by the very next API call without
 * requiring a page reload.
 */
const authMiddleware: Middleware = {
  async onRequest({ request }) {
    const apiKey = readCredential(SK.API_KEY);
    const adminKey = readCredential(SK.ADMIN_KEY);

    if (apiKey) request.headers.set("X-Api-Key", apiKey);
    if (adminKey) request.headers.set("X-Admin-Key", adminKey);

    return request;
  },
};

// ─── Error-parsing middleware ─────────────────────────────────────────────────

const errorMiddleware: Middleware = {
  async onResponse({ response }) {
    if (response.ok) return response;

    // Attempt to parse an RFC 9457 problem detail body.
    const contentType = response.headers.get("content-type") ?? "";
    let problem: ProblemDetail | null = null;

    if (
      contentType.includes("application/problem+json") ||
      contentType.includes("application/json")
    ) {
      try {
        const body = await response.clone().json();
        if (
          typeof body === "object" &&
          body !== null &&
          typeof (body as Record<string, unknown>).status === "number"
        ) {
          problem = body as ProblemDetail;
        }
      } catch {
        // Body wasn't valid JSON — leave problem as null.
      }
    }

    const message =
      problem?.detail ??
      problem?.title ??
      `HTTP ${response.status} ${response.statusText}`;

    throw new ApiError(response.status, problem, message);
  },
};

// ─── Client factory ───────────────────────────────────────────────────────────

export interface ClientOptions {
  /**
   * Base URL override.
   * When omitted the client reads sp_server_url from Web Storage at
   * request-time so it always reflects the latest settings.
   * Pass an explicit value in tests or for one-off requests (e.g. the
   * "Test connection" call on the Settings page).
   */
  baseUrl?: string;
  /** Override credentials (useful in tests or the settings test-connection flow). */
  apiKey?: string;
  adminKey?: string;
}

/**
 * Create a fully-typed openapi-fetch client.
 *
 * When `baseUrl` is omitted, the middleware reads `sp_server_url` from Web
 * Storage on every request so the URL always matches the current settings.
 *
 * @example
 * ```ts
 * const client = createApiClient();
 * const { data, error } = await client.GET("/v1/events", {
 *   params: { query: { page: 1, limit: 20 } },
 * });
 * ```
 */
export function createApiClient(options: ClientOptions = {}) {
  // When no explicit baseUrl is provided we create the client with an empty
  // string and inject the real URL per-request via middleware. This allows the
  // singleton to work correctly even if the user changes the server URL after
  // the module is first loaded.
  const client = createClient<paths>({ baseUrl: options.baseUrl ?? "" });

  if (options.baseUrl === undefined) {
    // Dynamic base URL: read from storage on every request.
    client.use({
      async onRequest({ request }) {
        const serverUrl = readCredential(SK.SERVER_URL);
        if (serverUrl) {
          const orig = new URL(request.url, "relative:///");
          // Replace origin with the stored server URL, keep path + query.
          const base = serverUrl.replace(/\/$/, "");
          const path = orig.pathname + orig.search + orig.hash;
          return new Request(base + path, request);
        }
        return request;
      },
    });
  }

  // Inject credentials.
  if (options.apiKey !== undefined || options.adminKey !== undefined) {
    // Explicit override (tests / settings test-connection).
    const key = options.apiKey ?? "";
    const adminKey = options.adminKey ?? "";
    client.use({
      async onRequest({ request }) {
        if (key) request.headers.set("X-Api-Key", key);
        if (adminKey) request.headers.set("X-Admin-Key", adminKey);
        return request;
      },
    });
  } else {
    client.use(authMiddleware);
  }

  client.use(errorMiddleware);
  return client;
}

// ─── Default singleton — used by the hooks in hooks.ts ───────────────────────

export const apiClient = createApiClient();

// Re-export path types for use in call sites.
export type { paths };
