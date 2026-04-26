# Soroban Pulse TypeScript SDK

Auto-generated TypeScript client for the Soroban Pulse API.

## Features

- Modern `fetch` API (no external dependencies)
- Full TypeScript support with typed request parameters and response models
- Support for versioned (`v1`) and deprecated endpoints

## Installation

```bash
npm install
# or
yarn install
```

## Usage

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

## SSE Streaming (Experimental)

The SDK provides model definitions for Events. For real-time streaming, use the standard `EventSource` API with the `/v1/events/contract/:id/stream` endpoint.
