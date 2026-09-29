# @soroban-pulse/feed-widget

A drop-in `<soroban-pulse-feed>` web component that shows recent activity for a Soroban contract,
backed by a [SorobanPulse](https://github.com/Soroban-Pulse/SorobanPulse) server. It works on any page
(plain HTML, React, Vue, Svelte, and others) and has no dependencies. It is about 4 KB gzipped.

## Quick start (CDN)

```html
<script type="module" src="https://cdn.jsdelivr.net/npm/@soroban-pulse/feed-widget@0.1.0/dist/feed-widget.js"></script>

<soroban-pulse-feed
  server="https://pulse.example.com"
  contract-id="CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"
  limit="10"
  live
></soroban-pulse-feed>
```

If you need a classic (non-module) script, use `dist/feed-widget.iife.js`. It is also available from
unpkg: `https://unpkg.com/@soroban-pulse/feed-widget@0.1.0`.

## npm

```bash
npm install @soroban-pulse/feed-widget
```

```js
import '@soroban-pulse/feed-widget'; // registers <soroban-pulse-feed>
```

## Attributes

| Attribute | Default | Description |
|---|---|---|
| `server` | — (required) | Base URL of the SorobanPulse API, for example `https://pulse.example.com`. |
| `contract-id` | — (required) | Contract to show events for (`C…` strkey). |
| `limit` | `10` | Number of events to show, from 1 to 50. |
| `theme` | `auto` | `light`, `dark` or `auto`. `auto` follows `prefers-color-scheme`. |
| `live` | off | When present, subscribes to the server's SSE stream and prepends new events. Use `live="false"` to turn it off explicitly. |
| `explorer-url` | — | Link template for transaction hashes, for example `https://stellar.expert/explorer/public/tx/{tx}`. Only `http(s)` URLs are used. |
| `heading` | `Recent activity` | Header text. |
| `api-key` | — | Sent as `X-Api-Key`. **Read [API keys](#api-keys) before using this.** |

Changing an attribute re-fetches the feed. You can also call `element.refresh()`, and read the
current list from `element.events`.

## DOM events

| Event | `detail` |
|---|---|
| `sp-feed-event` | The normalized event (`{ contractId, type, txHash, ledger, timestamp, topics, raw }`) whenever a live event arrives. |
| `sp-feed-error` | `{ message, status, error }` when loading or streaming fails. |

## Theming

Styles live in a Shadow DOM, so your page's CSS won't leak in. Customize the widget through CSS
custom properties set on the element:

```css
soroban-pulse-feed {
  --sp-feed-accent: #e5007a;
  --sp-feed-radius: 4px;
  --sp-feed-max-height: 24rem; /* scroll inside the widget */
  --sp-feed-font: "IBM Plex Sans", sans-serif;
}
```

| Property | Purpose |
|---|---|
| `--sp-feed-bg`, `--sp-feed-fg`, `--sp-feed-muted` | Surface, text and secondary text |
| `--sp-feed-border`, `--sp-feed-hover` | Dividers and row hover |
| `--sp-feed-accent`, `--sp-feed-accent-bg` | Links, contract-event badge, new-row highlight |
| `--sp-feed-system`, `--sp-feed-diagnostic` | Badge colours for system and diagnostic events |
| `--sp-feed-live`, `--sp-feed-error` | Live indicator and error text |
| `--sp-feed-radius`, `--sp-feed-max-height` | Corner radius and list height (defaults to `none`) |
| `--sp-feed-font`, `--sp-feed-font-mono` | Font stacks |

When `theme` is `dark` or `auto`, the dark defaults apply. Values you set yourself always win.
For structural changes, style the exposed parts: `::part(container)`, `::part(header)`,
`::part(list)`, `::part(item)` and `::part(footer)`.

The default colours come from the [SorobanPulse design tokens](../../design/).

## Embedding on a public site

The widget runs in your visitors' browsers and calls the SorobanPulse API directly. That has two
consequences that operators should plan for.

### CORS

The browser only lets the widget read responses if the SorobanPulse server allows your site's
origin:

```bash
# On the SorobanPulse server
ALLOWED_ORIGINS=https://mydapp.xyz,https://www.mydapp.xyz
```

- `ALLOWED_ORIGINS=*` is rejected when `ENVIRONMENT` is production-like, so list each embedding
  origin explicitly, including the scheme and any `www.` variant.
- The server allows `GET` with the `X-Api-Key` and `Last-Event-ID` headers and caches preflights
  for 24 h. Sending `api-key` triggers a preflight `OPTIONS` request, as does resuming a dropped
  live stream (`Last-Event-ID`). A first load without a key needs no preflight.
- Live mode uses `fetch` streaming (not `EventSource`), so the same CORS rules apply to the stream.
- If your site has a Content-Security-Policy, add the SorobanPulse server to `connect-src` and the
  CDN host (`https://cdn.jsdelivr.net`) to `script-src`.

### API keys

**Anything in your HTML is public.** An `api-key` attribute can be read by anyone who opens the page
source, so treat it as published. Choose one of these setups:

1. **A public read-only deployment (recommended).** Run a SorobanPulse instance (or replica) with no
   `API_KEY`, configured to index only the contracts you want to show, behind per-IP rate limiting
   (`RATE_LIMIT_PER_MINUTE`, plus `BEHIND_PROXY=true` when it sits behind a load balancer).
   Don't set `api-key` on the widget.
2. **A same-origin proxy that injects the key.** Point `server` at your own backend. The proxy
   forwards only `GET /v1/events/contract/<your-contract>` and `…/stream` and adds `X-Api-Key`
   server-side, so the key never reaches the browser. This also removes the need for CORS. nginx
   example:

   ```nginx
   location ~ ^/pulse/(v1/events/contract/YOUR_CONTRACT_ID(/stream)?)$ {
     limit_except GET { deny all; }
     proxy_pass https://pulse.internal/$1$is_args$args;
     proxy_set_header X-Api-Key $pulse_api_key;   # from a secret, not the page
     proxy_buffering off;                          # required for SSE
     proxy_read_timeout 1h;
   }
   ```
3. **A dedicated, low-privilege key per site.** If you must put a key in the page, issue one only for
   this widget, cap it with the per-key limits (`RATE_LIMIT_KEY_PER_MINUTE`/`_HOUR`/`_DAY`), and be
   ready to rotate it. In multi-tenant mode, map it to a tenant that contains only public data.
   **Never embed an admin key.**

### Live connections

Each open page with `live` holds one SSE connection. The server caps these with
`SSE_MAX_CONNECTIONS` and returns `503` when the cap is full. The widget then keeps showing the
last fetched events and retries with backoff. For high-traffic pages, consider leaving `live` off,
or raising the cap and putting the stream behind a proxy with SSE support.

## Browser support

Current versions of Chrome, Edge, Firefox and Safari (custom elements, Shadow DOM,
`TextDecoderStream`).

## Development

```bash
npm install
npm run build        # dist/ + d.ts, fails if a bundle exceeds 15 KB gzipped
open example/index.html
```

### Publishing

```bash
npm login            # member of the @soroban-pulse npm org
npm publish          # runs the build via prepublishOnly; publishConfig.access = public
```

After publishing, the CDN URLs above resolve automatically on jsDelivr and unpkg.
