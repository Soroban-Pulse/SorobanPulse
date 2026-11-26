import { createHmac, timingSafeEqual } from "crypto";
import { ApiError, AuthenticationError } from "./errors";
import { isRetryableStatusCode, withRetry } from "./retry";
import type {
  CreateSubscriptionParams,
  ListEventsParams,
  Page,
  SorobanEvent,
  SorobanPulseClientOptions,
  Subscription,
} from "./types";

const DEFAULT_BASE_URL = "https://api.sorobanpulse.io/v1";

/** Default tolerance (in seconds) for webhook signature timestamps. */
export const DEFAULT_WEBHOOK_TOLERANCE_SECONDS = 300;

export interface VerifyWebhookSignatureOptions {
  /** Raw request body exactly as received (string or bytes). */
  body: string | Uint8Array;
  /** Value of the `X-SorobanPulse-Signature` header (v2, preferred). */
  signature?: string | null;
  /** Value of the legacy `X-Signature-256` header (deprecated). */
  legacySignature?: string | null;
  /** Shared webhook secret. */
  secret: string;
  /** Allowed clock skew in seconds. Defaults to 300 (5 minutes). */
  toleranceSeconds?: number;
  /** Current unix time in seconds; injectable for testing. */
  now?: number;
}

function toBuffer(body: string | Uint8Array): Buffer {
  return typeof body === "string" ? Buffer.from(body, "utf8") : Buffer.from(body);
}

function hmacHex(secret: string, payload: string | Buffer): string {
  return createHmac("sha256", secret).update(payload).digest("hex");
}

function safeEqualHex(a: string, b: string): boolean {
  const bufA = Buffer.from(a, "hex");
  const bufB = Buffer.from(b, "hex");
  if (bufA.length !== bufB.length || bufA.length === 0) return false;
  return timingSafeEqual(bufA, bufB);
}

/**
 * Verify an incoming Soroban Pulse webhook signature.
 *
 * Supports signature v2 (`X-SorobanPulse-Signature: t=<unix>,v1=<hex>`) where
 * `v1 = HMAC-SHA256(secret, "<t>.<body>")`, and falls back to the legacy
 * `X-Signature-256` header (HMAC over the body only) during the deprecation
 * window. Returns `true` when the signature is valid and fresh.
 */
export function verifyWebhookSignature(options: VerifyWebhookSignatureOptions): boolean {
  const { body, secret, signature, legacySignature } = options;
  const tolerance = options.toleranceSeconds ?? DEFAULT_WEBHOOK_TOLERANCE_SECONDS;
  const now = options.now ?? Math.floor(Date.now() / 1000);
  const rawBody = toBuffer(body);

  if (signature) {
    const parts = signature.split(",").map((p) => p.trim());
    let timestamp: number | undefined;
    let provided: string | undefined;
    for (const part of parts) {
      const eq = part.indexOf("=");
      if (eq === -1) continue;
      const key = part.slice(0, eq);
      const value = part.slice(eq + 1);
      if (key === "t") timestamp = Number(value);
      else if (key === "v1") provided = value;
    }
    if (timestamp === undefined || !Number.isFinite(timestamp) || !provided) {
      return false;
    }
    if (Math.abs(now - timestamp) > tolerance) {
      return false;
    }
    const expected = hmacHex(secret, Buffer.concat([Buffer.from(`${timestamp}.`, "utf8"), rawBody]));
    return safeEqualHex(expected, provided);
  }

  if (legacySignature) {
    const provided = legacySignature.startsWith("sha256=")
      ? legacySignature.slice("sha256=".length)
      : legacySignature;
    return safeEqualHex(hmacHex(secret, rawBody), provided);
  }

  return false;
}

export class SorobanPulseClient {
  readonly baseUrl: string;
  private readonly apiKey: string;
  private readonly timeoutMs: number;
  private readonly maxRetries: number;

  constructor(options: SorobanPulseClientOptions) {
    if (!options.apiKey) {
      throw new AuthenticationError("apiKey is required");
    }
    this.apiKey = options.apiKey;
    this.baseUrl = (options.baseUrl ?? DEFAULT_BASE_URL).replace(/\/$/, "");
    this.timeoutMs = options.timeoutMs ?? 30_000;
    this.maxRetries = options.maxRetries ?? 3;
  }

  private headers(): Record<string, string> {
    return {
      Authorization: `Bearer ${this.apiKey}`,
      "Content-Type": "application/json",
      "User-Agent": "soroban-pulse-js-sdk/0.1.0",
    };
  }

  private async request<T>(
    method: string,
    path: string,
    params?: Record<string, string | number | undefined>,
    body?: unknown,
  ): Promise<T> {
    const url = new URL(`${this.baseUrl}${path}`);
    if (params) {
      for (const [key, value] of Object.entries(params)) {
        if (value !== undefined) url.searchParams.set(key, String(value));
      }
    }

    return withRetry<T>(
      async () => {
        const controller = new AbortController();
        const timeout = setTimeout(() => controller.abort(), this.timeoutMs);

        try {
          const response = await fetch(url.toString(), {
            method,
            headers: this.headers(),
            body: body !== undefined ? JSON.stringify(body) : undefined,
            signal: controller.signal,
          });

          const text = await response.text();
          const payload = text ? JSON.parse(text) : {};

          if (!response.ok) {
            if (response.status === 401 || response.status === 403) {
              throw new AuthenticationError(payload.message ?? "authentication failed");
            }
            throw new ApiError(response.status, payload.message ?? "request failed", payload);
          }

          return payload as T;
        } finally {
          clearTimeout(timeout);
        }
      },
      (err) => err instanceof ApiError && isRetryableStatusCode(err.statusCode),
      { maxRetries: this.maxRetries },
    );
  }

  async listEvents(params: ListEventsParams = {}): Promise<Page<SorobanEvent>> {
    return this.request<Page<SorobanEvent>>("GET", "/events", {
      contract_id: params.contractId,
      event_type: params.eventType,
      limit: params.limit,
      cursor: params.cursor,
    });
  }

  async *iterEvents(params: ListEventsParams = {}): AsyncGenerator<SorobanEvent> {
    let cursor = params.cursor;
    for (;;) {
      const page = await this.listEvents({ ...params, cursor });
      for (const event of page.data) {
        yield event;
      }
      if (!page.nextCursor) return;
      cursor = page.nextCursor;
    }
  }

  async getEvent(eventId: string): Promise<SorobanEvent> {
    return this.request<SorobanEvent>("GET", `/events/${eventId}`);
  }

  async listSubscriptions(): Promise<Subscription[]> {
    const page = await this.request<Page<Subscription>>("GET", "/subscriptions");
    return page.data;
  }

  async createSubscription(params: CreateSubscriptionParams): Promise<Subscription> {
    return this.request<Subscription>("POST", "/subscriptions", undefined, {
      contract_id: params.contractId,
      webhook_url: params.webhookUrl,
      event_types: params.eventTypes ?? [],
    });
  }

  async deleteSubscription(subscriptionId: string): Promise<void> {
    await this.request<void>("DELETE", `/subscriptions/${subscriptionId}`);
  }
}
