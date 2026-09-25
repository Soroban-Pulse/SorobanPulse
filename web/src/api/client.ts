/**
 * Typed openapi-fetch client for the SorobanPulse API.
 *
 * Responsibilities:
 *  - Inject the X-Api-Key auth header on every request
 *  - Parse RFC 9457 Problem Detail responses into a typed ApiError
 *  - Re-export the fully-typed path helpers so call sites get inference
 */

import createClient, { type Middleware } from "openapi-fetch";
import type { paths, components } from "./schema.d.ts";

// ---------------------------------------------------------------------------
// RFC 9457 typed error
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Auth middleware — injects X-Api-Key from env
// ---------------------------------------------------------------------------

/**
 * Reads VITE_API_KEY at runtime (set in .env.local during dev).
 * Returns undefined when no key is configured so public endpoints
 * still work without a key.
 */
function getApiKey(): string | undefined {
  // import.meta.env is the Vite env object; falls back gracefully in tests.
  return (
    (typeof import.meta !== "undefined" &&
      (import.meta.env as Record<string, string | undefined>)
        .VITE_API_KEY) ||
    undefined
  );
}

const authMiddleware: Middleware = {
  async onRequest({ request }) {
    const key = getApiKey();
    if (key) {
      request.headers.set("X-Api-Key", key);
    }
    return request;
  },
};

// ---------------------------------------------------------------------------
// Error-parsing middleware — turns 4xx/5xx into ApiError
// ---------------------------------------------------------------------------

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
        // Clone so the original body stream stays readable.
        const body = await response.clone().json();
        // Validate the minimum RFC 9457 fields.
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

// ---------------------------------------------------------------------------
// Client factory
// ---------------------------------------------------------------------------

export interface ClientOptions {
  /**
   * Base URL for the API.
   * Defaults to VITE_API_BASE_URL env var, then falls back to the Vite dev
   * proxy prefix `/api` so the proxy can rewrite it.
   */
  baseUrl?: string;
  /** Override the API key (useful in tests). */
  apiKey?: string;
}

/**
 * Create a fully-typed openapi-fetch client.
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
  const baseUrl =
    options.baseUrl ??
    (typeof import.meta !== "undefined"
      ? ((import.meta.env as Record<string, string | undefined>)
          .VITE_API_BASE_URL ?? "")
      : "");

  const client = createClient<paths>({ baseUrl });

  // Allow per-instance key override (e.g. in tests).
  if (options.apiKey !== undefined) {
    const key = options.apiKey;
    client.use({
      async onRequest({ request }) {
        if (key) request.headers.set("X-Api-Key", key);
        return request;
      },
    });
  } else {
    client.use(authMiddleware);
  }

  client.use(errorMiddleware);

  return client;
}

// ---------------------------------------------------------------------------
// Default singleton — used by the hooks in hooks.ts
// ---------------------------------------------------------------------------

export const apiClient = createApiClient();

// Re-export path types for use in call sites.
export type { paths };
