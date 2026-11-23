# Soroban Pulse TypeScript SDK

Auto-generated TypeScript client for the Soroban Pulse API.

## Features

- Modern `fetch` API (no external dependencies)
- Full TypeScript support with typed request parameters and response models
- Support for versioned (`v1`) and deprecated endpoints
- Built-in SSE streaming with automatic reconnection and Last-Event-ID support

## Installation

### From npm registry

```bash
npm install @soroban-pulse/sdk
# or
yarn add @soroban-pulse/sdk
```

### From local repository

```bash
npm install ./sdk/typescript
# or
yarn add ./sdk/typescript
```

## Usage

### REST API

```typescript
import { DefaultApi, Configuration } from "./index";

const config = new Configuration({
  basePath: "http://localhost:3000",
});

const api = new DefaultApi(config);

// Get events for a contract
async function main() {
  const events = await api.getEventsByContract({
    contractId: "C...",
  });
  console.log(events.data);
}

main();
```

### SSE Streaming

The SDK provides built-in support for Server-Sent Events (SSE) streaming with automatic reconnection and Last-Event-ID resumption.

#### Stream all events

```typescript
import { EventsApi, Configuration } from "./index";

const config = new Configuration({
  basePath: "http://localhost:3000",
});

const api = new EventsApi(config);

const stream = api.streamEventsSSE({
  apiKey: "your-api-key", // optional
  onMessage: (event) => {
    const data = JSON.parse(event.data);
    console.log("New event:", data);
  },
  onPing: (timestamp) => {
    console.log("Server ping:", timestamp);
  },
  onClose: () => {
    console.log("Stream closed by server");
  },
  onError: (error) => {
    console.error("Stream error:", error);
  },
  autoReconnect: true,
  maxReconnectAttempts: 5,
  reconnectDelayMs: 1000,
});

stream.connect();

// Later, disconnect
stream.disconnect();
```

#### Stream events for a specific contract

```typescript
const stream = api.streamEventsByContractSSE("CABC...", {
  apiKey: "your-api-key",
  onMessage: (event) => {
    const data = JSON.parse(event.data);
    console.log("New event for contract:", data);
  },
});

stream.connect();
```

#### Stream events for multiple contracts

```typescript
const stream = api.streamMultiEventsSSE(["CABC...", "CDEF..."], {
  apiKey: "your-api-key",
  onMessage: (event) => {
    const data = JSON.parse(event.data);
    console.log("New event:", data);
  },
});

stream.connect();
```

### SSE Features

- **Automatic Reconnection**: The stream automatically reconnects on connection loss with exponential backoff
- **Last-Event-ID Support**: Event IDs are persisted in `localStorage` (browser) or memory (Node.js) and sent on reconnect to resume from where you left off
- **Authentication**: Pass an `apiKey` to automatically add the `X-Api-Key` header
- **Custom Headers**: Add custom headers via the `headers` option
- **Event Callbacks**: Handle `onMessage`, `onPing`, `onClose`, and `onError` events
- **Manual Control**: Call `connect()` and `disconnect()` to manage the connection lifecycle

### SSE Event Types

The server emits three types of SSE events:

- **message**: Contains a JSON-serialized event object
- **ping**: Sent every 15 seconds to keep the connection alive (data is an RFC 3339 timestamp)
- **close**: Sent when the server is shutting down (clients should reconnect)

## Webhook Signature Verification

The SDK exports a `verifyWebhook` helper that validates incoming webhook
signatures using a constant-time comparison (`crypto.timingSafeEqual`) and an
optional timestamp tolerance check.

```typescript
import { verifyWebhook } from "@soroban-pulse/sdk";

const result = verifyWebhook(rawBody, headers, secret, {
  toleranceSec: 300, // reject signatures older than 5 minutes
});

if (!result.valid) {
  // result.reason explains why verification failed
  throw new Error(`Invalid webhook signature: ${result.reason}`);
}
```

`rawBody` must be the **raw request body** (string or `Buffer`), not the
parsed JSON — re-serializing the payload will change the bytes and break the
signature. `headers` is a plain object of request headers (header names are
matched case-insensitively).

### Express example

```typescript
import express from "express";
import { verifyWebhook } from "@soroban-pulse/sdk";

const app = express();
const secret = process.env.WEBHOOK_SECRET!;

// Use express.raw so the exact bytes are preserved for verification.
app.post(
  "/webhooks/soroban-pulse",
  express.raw({ type: "application/json" }),
  (req, res) => {
    const result = verifyWebhook(req.body, req.headers, secret, {
      toleranceSec: 300,
    });

    if (!result.valid) {
      return res.status(401).json({ error: result.reason });
    }

    const event = JSON.parse(req.body.toString("utf8"));
    // ... handle the event
    res.status(204).end();
  }
);
```

### Shared test vectors

The helper is validated against the cross-language fixture
[`sdk/testdata/webhook_vectors.json`](../testdata/webhook_vectors.json), which
is shared with the Go and Python SDKs. Use it to confirm your integration
verifies signatures identically across languages.
