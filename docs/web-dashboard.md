# Web Operational Dashboard

A React + TypeScript single-page app in `dashboard/` for operators to
monitor SorobanPulse's system health, subscriptions, and webhook delivery
status in real time.

## Stack

- React 18 + React Router for the frontend shell and routing.
- Vite for dev server / build tooling.
- Recharts for metrics visualization (events ingested, p99 latency).
- react-i18next + `Intl` for translations and locale-aware formatting
  (English and Spanish included).
- Vitest + Testing Library for component tests.

## Running locally

```bash
cd dashboard
npm install
npm run dev
```

The dev server proxies `/api/*` requests to `http://localhost:8080` (see
`dashboard/vite.config.ts`), matching the existing SorobanPulse API server.

### Against the full Docker Compose stack

`docker-compose.e2e.yml` builds the dashboard (`dashboard/Dockerfile`, nginx)
alongside the API, Postgres, the RPC stub and a one-shot `seed` job that loads
`tests/e2e/seed.sql` after the app has run its migrations:

```bash
docker compose -f docker-compose.e2e.yml up --build --wait
open http://localhost:5174
```

nginx strips the `/api` prefix before proxying to the app (`API_UPSTREAM`,
default `http://app:3000`) and disables buffering so the live SSE stream works.

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
- **Live stream** (`src/pages/LiveStreamPage.tsx`) — subscribes to
  `GET /api/v1/events/stream` (SSE), shows the 50 most recent events, can be
  paused, and opens each event in a detail drawer.
- **Subscription filters and detail** — search by contract/webhook, filter by
  status, and open a subscription in a detail drawer with recent deliveries.
- **Responsive layout** — sidebar collapses into a drawer below 768px, tables
  become card lists, and detail drawers go full-screen.
- **Accessibility** — keyboard-operable throughout with visible focus, labelled
  tables/filters, chart text alternatives, polite live regions, and
  AA-contrast light/dark themes. See `dashboard/CONTRIBUTING.md`.
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
| `GET /api/v1/events/stream` | Live event stream (Server-Sent Events) |

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
    components/          # Sidebar, ResponsiveTable, DetailDrawer, ChartFigure,
                         # EventFilterBar, EventTable, EventDrawer, ScValViewer,
                         # EmptyState, ErrorState, Skeleton*, TruncatedText, …
    hooks/               # useFocusTrap, useMediaQuery, useTheme
    i18n/                # i18next setup and Intl formatters (useFormat)
    locales/             # en.json, es.json translation catalogues
    pages/               # LoginPage, StatusDashboard, EventExplorerPage,
                         # EventDetailPage, SubscriptionsPage, WebhooksPage,
                         # LiveStreamPage
  tests/                 # Vitest + Testing Library specs
```

## Contributing

Accessibility, responsive-layout and translation guidelines (including how to
add a language) are in [`dashboard/CONTRIBUTING.md`](../dashboard/CONTRIBUTING.md).

## Follow-ups

- Wire `/api/auth/login` to the project's real authentication backend
  (currently a plain fetch expecting `{ token }`).
- Add role-based access control for destructive subscription/webhook
  actions (pause, delete) once the corresponding API endpoints exist.
- The browser `EventSource` API cannot send an `Authorization` header, so the
  live stream only works where `/v1/events/stream` is reachable without a
  bearer token (or behind a cookie-authenticated proxy).
