# JavaScript / TypeScript SDK

The official JS SDK lives in `sdk/javascript/` and provides a typed client
for the SorobanPulse REST API, SSE-based event subscriptions, webhook
signature verification, and automatic retry with exponential backoff.

## Installation

```bash
cd sdk/javascript
npm install
npm run build
```

## Quick start

```ts
import { SorobanPulseClient } from "@sorobanpulse/sdk";

const client = new SorobanPulseClient({ apiKey: process.env.SOROBAN_PULSE_API_KEY! });

for await (const event of client.iterEvents({ contractId: "CABC123", limit: 50 })) {
  console.log(event.id, event.eventType);
}
```

## Event subscriptions (SSE)

```ts
import { SorobanPulseClient, EventSubscription } from "@sorobanpulse/sdk";

const client = new SorobanPulseClient({ apiKey: "sp_live_..." });
const subscription = new EventSubscription(client, {
  contractId: "CABC123",
  eventTypes: ["transfer"],
});

subscription.onEvent((event) => console.log(event.eventType, event.data));
subscription.run();
```

Reconnection uses exponential backoff (capped at 30s) for up to
`maxReconnects` attempts (default 10).

## Webhook verification

```ts
import { verifyWebhookSignature, WebhookVerificationError } from "@sorobanpulse/sdk";

try {
  verifyWebhookSignature(rawBody, req.headers["x-sorobanpulse-signature"], webhookSecret);
} catch (err) {
  if (err instanceof WebhookVerificationError) {
    // reject the request
  }
}
```

See `sdk/javascript/examples/webhook-express-example.ts` for a full Express
integration.

## Retry and backoff

All `SorobanPulseClient` requests are automatically retried on network
failures and `429`/`5xx` responses using exponential backoff with jitter
(`sdk/javascript/src/retry.ts`). Configure via `maxRetries` in the client
constructor (default 3).

## TypeScript definitions

The SDK is written in TypeScript with `strict` mode enabled; `npm run
build` emits `.d.ts` declaration files alongside the compiled JS in
`dist/`. All public types (`SorobanEvent`, `Subscription`,
`ListEventsParams`, etc.) are exported from the package root.

## Examples

- `sdk/javascript/examples/list-events.ts` — async iteration over events.
- `sdk/javascript/examples/subscribe-events.ts` — live SSE subscription.
- `sdk/javascript/examples/webhook-express-example.ts` — verifying webhooks
  in an Express app.

## Testing

```bash
cd sdk/javascript
npm test
```

Tests cover webhook signature verification (valid/invalid/expired/tampered)
and the retry/backoff helper (`sdk/javascript/tests/`).

## API surface

| Export | Purpose |
|---|---|
| `SorobanPulseClient` | REST client with built-in retry/backoff |
| `EventSubscription` | SSE event stream consumer with reconnect |
| `verifyWebhookSignature` | HMAC-SHA256 webhook verification |
| `SorobanPulseError`, `ApiError`, `AuthenticationError` | Typed error classes |
