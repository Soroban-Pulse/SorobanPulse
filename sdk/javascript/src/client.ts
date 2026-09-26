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
