# Web dashboard

SorobanPulse ships a small web dashboard (`web/`) for browsing indexed events, filtering by
contract or type, watching the live SSE stream, and inspecting event JSON. The backend serves it at
**`/ui`**, so you don't need a separate static host.

```bash
docker run -p 3000:3000 -e DATABASE_URL=… -e STELLAR_RPC_URL=… soroban-pulse
open http://localhost:3000/ui
```

## Configuration

| Variable | Default | Description |
|---|---|---|
| `SERVE_DASHBOARD` | `false` (binary) / `true` (Docker image) | Serve the dashboard at `/ui`. |
| `DASHBOARD_DIR` | `web/dist` (binary) / `/app/web/dist` (image) | Directory containing the built `index.html` and `assets/`. |
| `DASHBOARD_CONNECT_SRC` | empty | Comma-separated extra origins the dashboard may call, for when users point it at another API host. These are added to the CSP `connect-src`. |

The feature can also be compiled out. The `dashboard` cargo feature is on by default:

```bash
cargo build --release --no-default-features --features classical-crypto
```

If `SERVE_DASHBOARD=true` but the binary was built without the feature, or `index.html` is missing,
the server logs a warning at startup and `/ui` is not served.

In Helm, toggle it with `dashboard.enabled` and `dashboard.connectSrc` (see the
[chart README](../helm/soroban-pulse/README.md#web-dashboard)).

## How it is served

| Path | Behaviour | `Cache-Control` |
|---|---|---|
| `/ui/assets/*` | Vite's content-hashed JS and CSS. A missing file returns `404`, not the app shell. | `public, max-age=31536000, immutable` |
| `/ui/*` (anything else) | Files in `DASHBOARD_DIR`, falling back to `index.html` (SPA routing) | `no-cache` |

Pre-compressed `.gz` and `.br` siblings are served when present, and the global compression layer
covers the rest.

## Security

**Authentication.** `/ui` is public, because it only serves static files. Every data request the
dashboard makes goes to the normal `/v1/*` API and needs an API key when `API_KEY` is set. Users
enter the key in the dashboard's **Settings** tab. It is kept in the browser's `localStorage` and
sent as the `X-Api-Key` header, never in URLs. `/ui` is also exempt from API rate limiting.

**Content-Security-Policy.** The dashboard gets its own policy instead of the API's
`default-src 'none'`:

```
default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:;
font-src 'self'; connect-src 'self' <DASHBOARD_CONNECT_SRC>; manifest-src 'self';
object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'
```

- No `'unsafe-inline'` or `'unsafe-eval'`. The Vite build emits no inline scripts (the module-preload
  polyfill is disabled), and all styles are external files.
- `frame-ancestors 'none'` and `X-Frame-Options: DENY` prevent clickjacking. The dashboard can't be
  embedded. For embedding on other sites, use the [feed widget](../packages/feed-widget/) instead.
- Event payloads are rendered with DOM text nodes, never `innerHTML`.
- The other OWASP headers (HSTS, `nosniff`, `Referrer-Policy`, `Permissions-Policy`) apply as they do
  to every other response.

## Development

```bash
cd web
npm install
SOROBAN_PULSE_API=http://localhost:3000 npm run dev   # proxies /v1 to the API
npm run build                                         # → web/dist
SERVE_DASHBOARD=true DASHBOARD_DIR=web/dist cargo run # serve the build from the backend
```

The dashboard is vanilla TypeScript with no framework. It is styled only with the shared
[design system](design/design-system.md) (`design/build/tokens.css` and `design/components.css`).
