# Soroban Pulse — Web UI

React + TypeScript single-page app for exploring indexed events, monitoring
system health and operating the indexer.
# Soroban Pulse UI

A React + TypeScript web UI for Soroban Pulse. It covers:

| Page | Route | Issue |
|---|---|---|
| Contract overview (summary cards, activity chart, event-type breakdown, event table, WASM timeline) | `/contracts/:id` | #1099 |
| Webhook subscriptions (list, create/edit, test, pause/resume/delete, one-time secret) | `/subscriptions` | #1100 |
| Delivery log per subscription, delivery detail, redeliver | `/subscriptions/:id` | #1101 |
| Dead-letter queue (filters, dry run, bulk replay with progress) | `/admin/dlq` | #1101 |
| Notification channels (all types, test, enable/disable, clone, groups, maintenance windows, sparklines) | `/channels` | #1102 |

## Running locally

```bash
cd frontend
npm install
npm run dev          # http://localhost:5173, proxies API calls to http://localhost:3000
```

Set `VITE_API_PROXY` to proxy to a different server in development, or
`VITE_API_BASE_URL` at build time to call a remote API directly (the server's
`ALLOWED_ORIGINS` must then include the UI origin). The base URL, API key and
admin key can also be changed at runtime from **Settings** (⚙); keys are stored
only in the browser's localStorage.

## Pages

| Route | Description |
| --- | --- |
| `/explorer` | Event list. Filters: `?ledger=`, `?search=` (full-text), `?contract_id=`, `?event_type=` |
| `/contracts/:id` | Contract summary and events |
| `/accounts/:id` | Events mentioning a G… / M… address (full-text search) |
| `/tx/:hash` | Events emitted by a transaction |
| `/status` | Health tiles, 24h history and SLO report. `?public=1` for the read-only public view |
| `/admin` | Indexer status, pause/resume, replay, gap backfill and audit log (admin key) |

## Global search

Press <kbd>/</kbd> or <kbd>⌘K</kbd> / <kbd>Ctrl K</kbd> anywhere. Input is classified in
[`src/lib/detect.ts`](src/lib/detect.ts):

| Input | Detected as | Goes to |
| --- | --- | --- |
| `C…` (56 chars, valid strkey checksum) | Contract | `/contracts/:id` |
| `G…` (56) / `M…` (69) | Account / muxed account | `/accounts/:id` |
| 64 hex chars (optional `0x`) | Transaction hash | `/tx/:hash` |
| Integer (≤ u32, `,`/`_` separators allowed) | Ledger | `/explorer?ledger=N` |
| Anything else | Free text | `/explorer?search=…` |

The detected destination is always the highlighted first option, so pasting an
ID and pressing Enter once navigates. Partial `C…` input autocompletes from
`/v1/contracts/search`. The last 8 searches are kept in localStorage.

## Theming

All colours are CSS variables in [`src/styles/tokens.css`](src/styles/tokens.css),
with a light and a dark set. The theme follows `prefers-color-scheme` until
the user picks Light or Dark with the header toggle (persisted as `sp.theme`).
Charts, code blocks and the JSON tree use the `--color-chart-*` and
`--color-code-*` tokens.

`npm run lint:colors` fails if a hex, `rgb()`/`hsl()` or named colour appears
anywhere outside `tokens.css`.

## Server features the UI expects but degrades without

- **Replay progress** — `POST /v1/admin/replay` currently returns no job id. If
  it starts returning `job_id` (or `status_url`), the admin console polls it
  for `status` / `progress`; until then jobs show as "accepted".
- **Gap detection** — the gaps panel reads `GET /v1/admin/indexer/gaps`
  (`{ data: [{ from_ledger, to_ledger, reason? }] }`) and shows a notice when
  the endpoint is missing. "Backfill" submits replay jobs in 10,000-ledger chunks.
npm run dev          # http://localhost:5173, proxies /v1 to http://localhost:3000
```

Set `SOROBAN_PULSE_API=http://host:port` to proxy to another API. Open **Settings** in the UI to enter
`API_KEY` / `ADMIN_API_KEY`, and say whether the server runs in production or staging (HTTPS-only callback URLs).
Settings are stored in the browser's localStorage.

`npm run build` type-checks and writes a static bundle to `dist/`.

## Layout

```
src/
  api/          fetch client, response types, one module per API area
  components/   shared UI (modal, confirm dialog, badges, toasts) and SVG charts
  features/
    channels/   schema.ts holds the per-type config schema; forms/ has one component per channel type
    contracts/
    deliveries/
    subscriptions/
  lib/          formatting, validation (mirrors the server's SSRF rules), useAsync
```

To add a channel type, add its schema to `features/channels/schema.ts`, add a form component in
`features/channels/forms/`, register it in `forms/index.ts`, and add a glyph in `ChannelIcon.tsx`.

## API endpoints used

Endpoints marked **pending** are not in `src/routes.rs` yet. The UI calls them with the shapes in
`src/api/types.ts` and shows an inline error until the backend work lands.

### Contracts (#1099)

| Method | Path | Status |
|---|---|---|
| GET | `/v1/contracts/{id}/summary` | available |
| GET | `/v1/contracts/{id}/stats/history?days=7\|30` | available |
| GET | `/v1/events/timeseries?contract_id=&bucket=1h&from_ledger=` | available |
| GET | `/v1/events/contract/{id}?page=&limit=&exact_count=false` | available |
| GET | `/v1/contracts/search?q=` | available (label, metadata and `sac_asset` shown when the server returns them) |
| GET | `/v1/contracts/{id}/wasm-versions` | pending, section is hidden until it exists |

### Subscriptions and deliveries (#1100, #1101)

| Method | Path | Status |
|---|---|---|
| POST | `/v1/subscriptions` | available (`contract_ids`, `event_types` and `secret` are sent as well) |
| GET / DELETE | `/v1/subscriptions/{id}` | available |
| POST | `/v1/subscriptions/{id}/pause`, `/resume` | available |
| GET | `/v1/subscriptions?status=&contract_id=&q=` | pending |
| PATCH | `/v1/subscriptions/{id}` | pending |
| POST | `/v1/subscriptions/{id}/test` | pending |
| GET | `/v1/subscriptions/{id}/deliveries?status=&page=&page_size=` | pending |
| GET | `/v1/subscriptions/{id}/deliveries/{delivery_id}` | pending |
| POST | `/v1/subscriptions/{id}/deliveries/{delivery_id}/redeliver` | pending |
| GET | `/v1/admin/webhooks/dlq?endpoint_url=&failure_reason=&created_after=&created_before=&max_attempts=` | pending |
| POST | `/v1/admin/webhooks/dlq/replay` with `dry_run` (or `ids`) → `{ matched, replayed, job_id }` | pending |
| GET | `/v1/admin/webhooks/dlq/replay/{job_id}` → `{ total, processed, succeeded, failed, done }` | pending |

### Notification channels (#1102)

| Method | Path | Status |
|---|---|---|
| POST | `/v1/admin/notifications/channels` | available (accepts every channel type) |
| GET | `/v1/admin/notifications/channels?q=&channel_type=&status=&tag=` | pending (params match `NotificationChannelSearchParams`) |
| PATCH / DELETE | `/v1/admin/notifications/channels/{id}` | pending (body matches `UpdateNotificationChannelRequest`) |
| POST | `/v1/admin/notifications/channels/{id}/test` | handler exists (`test_notification_channel`), route pending |
| GET | `/v1/admin/notifications/channels/health` | pending, falls back to the success rate from `/dashboard` |
| GET | `/v1/admin/notifications/channels/{id}/stats?period=7d` → `[{ bucket_start, sent, failed }]` | pending |
| GET | `/v1/admin/notifications/dashboard` | handler exists, route pending |
| GET | `/v1/admin/notifications/channel-groups` | pending |
| GET / POST / DELETE | `/v1/admin/notifications/maintenance-windows` | handlers exist in `notification_admin.rs`, routes pending |
