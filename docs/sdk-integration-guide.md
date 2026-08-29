# SDK Integration Guide

This guide covers integrating the Soroban Pulse SDKs into your application. SDKs are available for TypeScript/JavaScript, Python, and Go.

Building or extending the SDKs themselves rather than consuming them? See the [SDK Development Guide](sdk-development.md) for architecture, per-language contributor tutorials, and webhook verification internals.

## Table of Contents

- [Quickstart](#quickstart)
- [Authentication](#authentication)
- [Subscription Patterns](#subscription-patterns)
- [Error Handling](#error-handling)
- [Streaming Events](#streaming-events)
- [TypeScript / JavaScript Guide](#typescript--javascript-guide)
- [Python Guide](#python-guide)
- [Go Guide](#go-guide)

---

## Quickstart

All three SDKs follow the same pattern: create a configuration object pointing at your Soroban Pulse instance, instantiate an API client, and call methods.

### TypeScript

```typescript
import { EventsApi, Configuration } from "./sdk/typescript";

const api = new EventsApi(new Configuration({
  basePath: "http://localhost:3000",
  apiKey: "your-api-key", // omit if API_KEY is not set on the server
}));

const response = await api.getEvents({ page: 1, limit: 20 });
console.log(response.data);
```

### Python

```python
import openapi_client
from openapi_client import EventsApi, Configuration

config = Configuration(host="http://localhost:3000")
async with openapi_client.ApiClient(config) as client:
    api = EventsApi(client)
    response = await api.get_events(page=1, limit=20)
    print(response.data)
```

### Go

```go
import sp "github.com/soroban-pulse/client-go"

client := sp.NewClient(sp.ClientConfig{
    BaseURL: "http://localhost:3000",
    APIKey:  "your-api-key",
})
defer client.Close()

events, err := client.GetEvents(ctx, sp.NewGetEventsOptions())
```

---

## Authentication

Soroban Pulse supports two optional authentication mechanisms. Both are disabled when the corresponding environment variable is unset on the server.

### Regular API key (`API_KEY`)

Protects all endpoints except `/health` and `/healthz/*`.

Send the key in either header — both are accepted:
```
Authorization: Bearer <API_KEY>
X-Api-Key: <API_KEY>
```

#### TypeScript
```typescript
const config = new Configuration({
  basePath: "http://localhost:3000",
  apiKey: process.env.SOROBAN_PULSE_API_KEY,
});
```

#### Python
```python
config = Configuration(
    host="http://localhost:3000",
    api_key={"ApiKey": os.environ["SOROBAN_PULSE_API_KEY"]},
)
```

#### Go
```go
client := sp.NewClient(sp.ClientConfig{
    BaseURL: "http://localhost:3000",
    APIKey:  os.Getenv("SOROBAN_PULSE_API_KEY"),
})
```

### Admin API key (`ADMIN_API_KEY`)

Required for `/v1/admin/*` endpoints. The admin key is completely separate from the regular `API_KEY`. A regular key sent to an admin endpoint returns `403 Forbidden`; no key at all returns `401 Unauthorized`.

```bash
# Example admin request with curl
curl -H "X-Api-Key: $ADMIN_API_KEY" \
     http://localhost:3000/v1/admin/indexer/pause
```

---

## Subscription Patterns

### Poll for new events (simple)

Suitable for low-frequency use cases where real-time delivery is not required.

#### TypeScript
```typescript
async function pollForNewEvents(api: EventsApi, afterLedger: number) {
  const response = await api.getEvents({
    page: 1,
    limit: 100,
    fromLedger: afterLedger,
  });

  for (const event of response.data) {
    process(event);
  }

  // Return the highest ledger seen for the next poll
  return response.data.reduce(
    (max, e) => Math.max(max, e.ledger),
    afterLedger,
  );
}
```

#### Python
```python
async def poll_for_new_events(api: EventsApi, after_ledger: int) -> int:
    response = await api.get_events(page=1, limit=100, from_ledger=after_ledger)
    for event in response.data:
        process(event)
    if response.data:
        return max(e.ledger for e in response.data)
    return after_ledger
```

### Real-time streaming (SSE)

Server-Sent Events deliver new events within one poll cycle (~5 seconds) of them being indexed.

#### TypeScript
```typescript
import { EventsApi, Configuration } from "./sdk/typescript";

const api = new EventsApi(new Configuration({
  basePath: "http://localhost:3000",
  apiKey: "your-api-key",
}));

// Stream all events
const stream = api.streamEventsSSE({
  onMessage: (event) => {
    const data = JSON.parse(event.data);
    console.log("New event:", data.id, "at ledger", data.ledger);
  },
  onPing: (timestamp) => console.log("Keepalive:", timestamp),
  onClose: () => console.log("Server shutting down"),
  onError: (err) => console.error("Stream error:", err),
  autoReconnect: true,
  maxReconnectAttempts: 10,
  reconnectDelayMs: 1000,
});

stream.connect();

// Stop later
// stream.disconnect();
```

#### Python
```python
import asyncio
import aiohttp

async def stream_events():
    async with aiohttp.ClientSession() as session:
        async with session.get(
            "http://localhost:3000/v1/events/stream",
            headers={"X-Api-Key": "your-api-key"},
        ) as resp:
            async for line in resp.content:
                line = line.decode().strip()
                if line.startswith("data:"):
                    data = json.loads(line[5:].strip())
                    process(data)

asyncio.run(stream_events())
```

#### Go
```go
err := client.StreamEvents(ctx, nil, func(event *sp.Event) error {
    fmt.Printf("Event %s at ledger %d\n", event.ID, event.Ledger)
    return nil
})
```

### Contract-specific subscription

Filter events at the server to reduce bandwidth:

#### TypeScript
```typescript
// Single contract
const stream = api.streamEventsByContractSSE("CABC...", {
  onMessage: (event) => console.log(JSON.parse(event.data)),
});
stream.connect();

// Multiple contracts over one connection
const multiStream = api.streamMultiEventsSSE(["CABC...", "CDEF..."], {
  onMessage: (event) => console.log(JSON.parse(event.data)),
});
multiStream.connect();
```

#### Go
```go
contractID := "CABC..."
err := client.StreamEvents(ctx, &contractID, func(event *sp.Event) error {
    fmt.Println("Contract event:", event.ID)
    return nil
})
```

---

## Error Handling

All three SDKs implement automatic retry with exponential backoff for transient errors. The default backoff sequence is: **1 s → 2 s → 4 s → 8 s → 16 s** (retries on HTTP 429, 500, 502, 503, 504).

### TypeScript

```typescript
const config = new Configuration({
  basePath: "http://localhost:3000",
  maxRetries: 3,
  retryOnStatus: [429, 500, 502, 503, 504],
  retryInitialDelayMs: 1000,
  retryMaxDelayMs: 32000,
  onRetry: (attempt, delayMs, reason) => {
    console.warn(`Retry ${attempt}: ${reason} — waiting ${delayMs}ms`);
  },
});

const api = new EventsApi(config);

try {
  const response = await api.getEvents({ page: 1, limit: 20 });
} catch (error) {
  // Thrown only after all retries are exhausted
  if (error instanceof Response) {
    console.error(`HTTP ${error.status}: ${await error.text()}`);
  } else {
    console.error("Network error:", error);
  }
}
```

**Retry presets**:
- `maxRetries: 3` — default; good for most workloads
- `maxRetries: 5` — aggressive; use for critical operations
- `maxRetries: 1` — conservative; use when you prefer fast failure

### Python

```python
from openapi_client import ApiException

try:
    response = await api.get_events(page=1, limit=20)
except ApiException as e:
    if e.status == 429:
        print("Rate limited — back off and retry")
    elif e.status == 404:
        print("Contract not found")
    elif e.status >= 500:
        print(f"Server error {e.status}: {e.body}")
    else:
        raise
```

Configure retry behaviour via `RetryPolicyConfig`:
```python
from openapi_client import RetryPolicyConfig, create_aggressive_retry_policy

# Use a preset
policy = create_aggressive_retry_policy()  # max 5 retries, up to 60 s backoff

# Or custom
policy = RetryPolicyConfig(
    max_retries=3,
    initial_delay=1.0,
    max_delay=32.0,
    retry_on_status=[429, 500, 503],
)
```

### Go

```go
events, err := client.GetEvents(ctx, opts)
if err != nil {
    switch {
    case err == context.DeadlineExceeded:
        log.Println("Request timed out")
    case err == context.Canceled:
        log.Println("Request cancelled")
    default:
        log.Printf("API error: %v", err)
    }
    return
}
```

Configure retry in `ClientConfig`:
```go
client := sp.NewClient(sp.ClientConfig{
    BaseURL:              "http://localhost:3000",
    MaxRetries:           3,
    RetryInitialDelay:    time.Second,
    RetryMaxDelay:        32 * time.Second,
    RetryableStatusCodes: []int{429, 500, 502, 503, 504},
    OnRetry: func(attempt int, delay time.Duration, reason string) {
        log.Printf("Retry %d: %s (waiting %v)", attempt, reason, delay)
    },
})
```

### HTTP error reference

| Status | Meaning | Action |
|--------|---------|--------|
| `400` | Invalid query parameter | Fix the request — do not retry |
| `401` | Missing API key | Set the `API_KEY` / `ADMIN_API_KEY` header |
| `403` | Wrong key tier | Use `ADMIN_API_KEY` for admin endpoints |
| `404` | Resource not found | Verify contract ID or tx hash |
| `429` | Rate limited | Back off; raise `RATE_LIMIT_PER_MINUTE` if you own the server |
| `503` | Service unavailable | Retry with backoff; check `/healthz/ready` |

---

## Streaming Events

### SSE reconnection and Last-Event-ID

When a client reconnects after a disconnect, the server replays any events missed since the last received event ID. The TypeScript SDK stores the last event ID in `localStorage` (browser) or memory (Node.js) automatically.

```typescript
const stream = api.streamEventsSSE({
  autoReconnect: true,
  maxReconnectAttempts: 20,
  reconnectDelayMs: 2000,
  onMessage: (event) => {
    // event.lastEventId is automatically tracked and sent on reconnect
    handle(JSON.parse(event.data));
  },
});
stream.connect();
```

### Keep-alive pings

The server emits `event: ping` every `SSE_KEEPALIVE_SECS` seconds (default: 15). This prevents reverse proxies from closing idle connections. You can use the ping to detect a stale connection:

```typescript
let lastPing = Date.now();

const stream = api.streamEventsSSE({
  onPing: () => { lastPing = Date.now(); },
  onMessage: (event) => handle(JSON.parse(event.data)),
});

// Watchdog: reconnect if no ping in 45 seconds
setInterval(() => {
  if (Date.now() - lastPing > 45_000) {
    stream.disconnect();
    stream.connect();
  }
}, 10_000);
```

### NDJSON export (batch)

For one-off bulk exports, request NDJSON instead of SSE. Each line is a complete JSON event that can be processed as it arrives:

```bash
curl -H "Accept: application/x-ndjson" \
     -H "Authorization: Bearer $API_KEY" \
     http://localhost:3000/v1/events/export
```

```typescript
const response = await fetch("http://localhost:3000/v1/events", {
  headers: { Accept: "application/x-ndjson" },
});
for await (const line of response.body) {
  const event = JSON.parse(line);
  process(event);
}
```

---

## TypeScript / JavaScript Guide

### Installation

```bash
# From the sdk/typescript directory
npm install
```

The SDK uses the native `fetch` API with no external runtime dependencies.

### Module structure

| File | Purpose |
|------|---------|
| `index.ts` | Public exports |
| `runtime.ts` | HTTP client, retry logic |
| `sse.ts` | SSE streaming with reconnection |
| `interceptors.ts` | Request/response middleware hooks |
| `retry-policy.ts` | Retry and backoff configuration |
| `webhook-verification.ts` | HMAC signature verification for webhooks |
| `apis/` | Generated API classes |
| `models/` | Generated TypeScript types |

### Interceptors

Interceptors let you inject logic before every request or after every response — useful for logging, metrics, or custom auth headers:

```typescript
import { RequestContext, ResponseContext } from "./sdk/typescript";

const config = new Configuration({
  basePath: "http://localhost:3000",
  middleware: [
    {
      pre: async (ctx: RequestContext) => {
        console.log(`→ ${ctx.init.method} ${ctx.url}`);
        return ctx;
      },
      post: async (ctx: ResponseContext) => {
        console.log(`← ${ctx.response.status}`);
        return ctx.response;
      },
    },
  ],
});
```

### Webhook signature verification

```typescript
import { verifyWebhookSignature } from "./sdk/typescript/webhook-verification";

app.post("/webhook", (req, res) => {
  const isValid = verifyWebhookSignature(
    req.body,
    req.headers["x-webhook-signature"] as string,
    process.env.WEBHOOK_SECRET!,
  );
  if (!isValid) return res.status(401).send("Invalid signature");
  handle(req.body);
  res.sendStatus(200);
});
```

### TypeScript types

All response shapes are fully typed. Key types:

```typescript
interface Event {
  id: string;
  contract_id: string;
  event_type: "contract" | "diagnostic" | "system";
  tx_hash: string;
  ledger: number;
  timestamp: string;   // RFC 3339
  event_data: { value: unknown; topic: unknown[] };
  created_at: string;
}

interface PaginatedEvents {
  data: Event[];
  total: number;
  page: number;
  limit: number;
  approximate: boolean;
}
```

---

## Python Guide

### Installation

```bash
pip install -r sdk/python/requirements.txt
# or
pip install git+https://github.com/Soroban-Pulse/SorobanPulse.git#subdirectory=sdk/python
```

Requires Python 3.9+.

### Async usage

The Python SDK is async-first using `asyncio`:

```python
import asyncio
import openapi_client
from openapi_client import EventsApi, Configuration

async def main():
    config = Configuration(host="http://localhost:3000")
    async with openapi_client.ApiClient(config) as client:
        api = EventsApi(client)
        response = await api.get_events(page=1, limit=50)
        for event in response.data:
            print(f"{event.id} — ledger {event.ledger}")

asyncio.run(main())
```

### Filtering events

```python
# Filter by contract
events = await api.get_events_by_contract(contract_id="CABC...")

# Filter by ledger range
events = await api.get_events(
    page=1, limit=100,
    from_ledger=1_000_000,
    to_ledger=1_001_000,
)

# Filter by event type
events = await api.get_events(
    page=1, limit=100,
    event_type="contract",  # "contract", "diagnostic", or "system"
)

# Filter by transaction hash
events = await api.get_events_by_tx_hash(tx_hash="abc123...")
```

### Paginate through all events

```python
async def fetch_all_events(api: EventsApi, contract_id: str):
    page = 1
    while True:
        response = await api.get_events_by_contract(
            contract_id=contract_id,
            page=page,
            limit=100,
        )
        yield from response.data
        if page * response.limit >= response.total:
            break
        page += 1
```

### Retry policy presets

```python
from openapi_client import (
    create_default_retry_policy,    # 3 retries, 1–32 s backoff
    create_aggressive_retry_policy, # 5 retries, 500 ms–60 s backoff
    create_conservative_retry_policy, # 1 retry, 2–5 s backoff
)
```

---

## Go Guide

### Installation

```bash
go get github.com/soroban-pulse/client-go
```

Requires Go 1.21+. Uses only the standard library (no external dependencies).

### Client lifecycle

Always close the client when done to release connections:

```go
client := sp.NewClient(sp.ClientConfig{
    BaseURL: "http://localhost:3000",
    APIKey:  os.Getenv("SOROBAN_PULSE_API_KEY"),
    Timeout: 30 * time.Second,
})
defer client.Close()
```

### Context and timeouts

All methods accept a `context.Context`. Use it to set per-request deadlines:

```go
ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
defer cancel()

events, err := client.GetEvents(ctx, &sp.GetEventsOptions{
    Page:  1,
    Limit: 50,
})
```

### Full example: process new events in a loop

```go
package main

import (
    "context"
    "fmt"
    "log"
    "time"

    sp "github.com/soroban-pulse/client-go"
)

func main() {
    client := sp.NewClient(sp.ClientConfig{
        BaseURL:    "http://localhost:3000",
        MaxRetries: 3,
    })
    defer client.Close()

    var fromLedger int64 = 0

    for {
        ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)

        events, err := client.GetEvents(ctx, &sp.GetEventsOptions{
            Page:       1,
            Limit:      100,
            FromLedger: fromLedger,
        })
        cancel()

        if err != nil {
            log.Printf("Error fetching events: %v — retrying in 5s", err)
            time.Sleep(5 * time.Second)
            continue
        }

        for _, event := range events.Data {
            fmt.Printf("Event %s at ledger %d\n", event.ID, event.Ledger)
            if event.Ledger > fromLedger {
                fromLedger = event.Ledger
            }
        }

        time.Sleep(5 * time.Second)
    }
}
```

### Connection pooling

The Go client reuses HTTP connections automatically. Reuse one client instance across requests rather than creating a new one per call:

```go
// Good: one client, many calls
client := sp.NewClient(config)
for range requests {
    client.GetEvents(ctx, opts)
}

// Bad: new client per call (leaks connections)
for range requests {
    sp.NewClient(config).GetEvents(ctx, opts)
}
```

---

## Related Documentation

- [API Reference (Swagger UI)](http://localhost:3000/docs) — live on a running instance
- [OpenAPI Spec](../openapi.json) — machine-readable
- [Webhook Signing](webhook_signing.md) — HMAC signature verification
- [SSE Reconnection](sse-reconnection.md) — deep dive on Last-Event-ID
- [TypeScript SDK RETRY_AND_BACKOFF.md](../sdk/typescript/RETRY_AND_BACKOFF.md)
- [Python SDK RETRY_AND_BACKOFF.md](../sdk/python/RETRY_AND_BACKOFF.md)
- [Troubleshooting Guide](troubleshooting.md)
