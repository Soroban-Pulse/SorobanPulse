import type { SorobanPulseClient } from "./client";
import type { EventHandler, SorobanEvent } from "./types";

export interface EventSubscriptionOptions {
  contractId?: string;
  eventTypes?: string[];
  maxReconnects?: number;
}

/**
 * Consumes the SorobanPulse Server-Sent Events (SSE) stream, dispatching
 * parsed events to registered handlers, with automatic reconnection using
 * exponential backoff.
 */
export class EventSubscription {
  private handlers: EventHandler[] = [];
  private stopped = false;

  constructor(
    private readonly client: SorobanPulseClient,
    private readonly options: EventSubscriptionOptions = {},
  ) {}

  onEvent(handler: EventHandler): void {
    this.handlers.push(handler);
  }

  stop(): void {
    this.stopped = true;
  }

  private streamUrl(): string {
    const url = new URL(`${this.client.baseUrl}/events/stream`);
    if (this.options.contractId) url.searchParams.set("contract_id", this.options.contractId);
    for (const eventType of this.options.eventTypes ?? []) {
      url.searchParams.append("event_type", eventType);
    }
    return url.toString();
  }

  private dispatch(raw: string): void {
    const event = JSON.parse(raw) as SorobanEvent;
    for (const handler of this.handlers) handler(event);
  }

  /** Blocking async loop; call without awaiting to run in the background. */
  async run(): Promise<void> {
    const maxReconnects = this.options.maxReconnects ?? 10;
    let attempts = 0;

    while (!this.stopped && attempts < maxReconnects) {
      try {
        const response = await fetch(this.streamUrl(), {
          headers: { Authorization: `Bearer ${(this.client as unknown as { apiKey: string }).apiKey ?? ""}` },
        });
        if (!response.body) throw new Error("no response body for SSE stream");

        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = "";

        for (;;) {
          if (this.stopped) return;
          const { done, value } = await reader.read();
          if (done) break;

          buffer += decoder.decode(value, { stream: true });
          const lines = buffer.split("\n");
          buffer = lines.pop() ?? "";

          for (const line of lines) {
            if (line.startsWith("data:")) {
              this.dispatch(line.slice("data:".length).trim());
            }
          }
        }
        attempts = 0;
      } catch {
        attempts += 1;
        const backoffMs = Math.min(2 ** attempts * 1000, 30_000);
        await new Promise((resolve) => setTimeout(resolve, backoffMs));
      }
    }
  }
}
