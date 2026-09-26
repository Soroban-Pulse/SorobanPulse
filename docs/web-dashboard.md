# Web Operational Dashboard

A React + TypeScript single-page app in `dashboard/` for operators to
monitor SorobanPulse's system health, subscriptions, and webhook delivery
status in real time.

## Stack

- React 18 + React Router for the frontend shell and routing.
- Vite for dev server / build tooling.
- Recharts for metrics visualization (events ingested, p99 latency).
- Vitest + Testing Library for component tests.

## Running locally

```bash
cd dashboard
npm install
npm run dev
```

The dev server proxies `/api/*` requests to `http://localhost:8080` (see
`dashboard/vite.config.ts`), matching the existing SorobanPulse API server.

## Features

- **System status dashboard** (`src/pages/StatusDashboard.tsx`) — polls
  `/api/status` and `/api/metrics` every 15s, rendering health, uptime,
  version, and time-series charts for ingestion throughput and latency.
- **Event Explorer** (`src/pages/EventExplorerPage.tsx`) — searchable,
  filterable table of indexed Soroban events with pagination, type badges,
  and a detail drawer for inspecting event data and ScVal payloads.
- **Event Detail** (`src/pages/EventDetailPage.tsx`) — full-screen detail
  view for an individual event, showing summary metadata, ScVal data in
  tree/table/code views, and raw JSON.
- **Subscription management** (`src/pages/SubscriptionsPage.tsx`) — lists
  active/paused/failing subscriptions with their contract ID, webhook URL,
  and subscribed event types.
- **Webhook management** (`src/pages/WebhooksPage.tsx`) — per-subscription
  delivery history with attempt number, HTTP status code, and timestamp.
- **Authentication** (`src/auth/AuthContext.tsx`, `RequireAuth.tsx`) —
  email/password login against `/api/auth/login`, session token persisted
  in `localStorage`, all dashboard routes gated behind `RequireAuth`.

## API contract

The dashboard expects the following endpoints on the SorobanPulse API
server (see `dashboard/src/api/client.ts` for the exact shapes):

| Endpoint | Purpose |
|---|---|
| `POST /api/auth/login` | Exchange email/password for a bearer token |
| `GET /api/status` | Overall system health, uptime, version |
| `GET /api/metrics?range=<minutes>` | Time-series ingestion/latency metrics |
| `GET /api/events` | List events with filter, sort, and pagination params |
| `GET /api/events/:id` | Fetch a single event by ID |
| `GET /api/subscriptions` | List subscriptions and their status |
| `GET /api/subscriptions/:id/deliveries` | Webhook delivery history |

## Testing

```bash
cd dashboard
npm test
```

`dashboard/tests/AuthContext.test.tsx` covers login/logout/session
persistence; `dashboard/tests/StatTile.test.tsx` covers the stat tile
component used throughout the status dashboard.

## Folder structure

```
dashboard/
  src/
    api/
      client.ts          # typed fetch wrappers for the dashboard API
      eventTypes.ts      # TypeScript interfaces for event data
    auth/                # AuthContext + RequireAuth route guard
    components/          # NavBar, StatTile, EventFilterBar, EventTable,
                         # EmptyState, EventDrawer, ScValViewer, TruncatedText
    pages/               # LoginPage, StatusDashboard, SubscriptionsPage,
                         # WebhooksPage, EventExplorerPage, EventDetailPage
  tests/                 # Vitest + Testing Library specs
```

## Follow-ups

- Wire `/api/auth/login` to the project's real authentication backend
  (currently a plain fetch expecting `{ token }`).
- Add role-based access control for destructive subscription/webhook
  actions (pause, delete) once the corresponding API endpoints exist.
