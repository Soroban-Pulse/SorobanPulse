# API Guide

Entry point for the Soroban Pulse API surface. This page ties together the
REST API, GraphQL, Server-Sent Events, and webhooks/subscriptions, and links
out to the detailed docs for each rather than repeating them.

## Table of contents

1. [API surfaces overview](#api-surfaces-overview)
2. [Authentication](#authentication)
3. [Testing against the live Swagger UI](#testing-against-the-live-swagger-ui)
4. [Versioning](#versioning)
5. [How the OpenAPI spec is produced](#how-the-openapi-spec-is-produced)
6. [Where to go next](#where-to-go-next)

---

## API surfaces overview

| Surface | Transport | Default path | Docs |
|---|---|---|---|
| REST | HTTP request/response | `/v1/*` (plus unversioned legacy paths) | [`docs/api-usage.md`](api-usage.md) |
| Server-Sent Events | HTTP streaming (`text/event-stream`) | `/v1/events/stream`, `/v1/events/contract/{contract_id}/stream` | [`docs/api-usage.md#server-sent-events-sse`](api-usage.md#server-sent-events-sse), [`docs/sse-reconnection.md`](sse-reconnection.md) |
| Webhooks / subscriptions | HTTP callbacks the server initiates | `/subscriptions` (management) | [`docs/webhook_signing.md`](webhook_signing.md), [`docs/subscription-best-practices.md`](subscription-best-practices.md) |
| GraphQL | HTTP POST, single endpoint | `/graphql` (see caveat below) | [`docs/graphql_api.md`](graphql_api.md) |

All four sit on the same Axum application and (aside from the GraphQL
caveat below) the same port — there's no separate service to stand up for
any of them. Pick REST for simple request/response integrations, SSE for a
live tail of one contract's events, webhooks/subscriptions when you want the
server to push to you instead of polling, and GraphQL when you need to shape
a response across multiple related fields in one round trip.

> **GraphQL status**: `src/graphql.rs` defines a real `async-graphql` schema
> (`create_schema()`), gated behind the `graphql` Cargo feature, but the
> route wiring in `src/routes.rs` (`graphql_routes()`) currently returns an
> empty `Router::new()` rather than mounting that schema. In the current
> build, `/graphql` is **not reachable** — [`docs/graphql_api.md`](graphql_api.md)
> describes the intended interface once that wiring is completed, not
> necessarily what responds today. Verify against a running instance before
> relying on it.

## Authentication

Authentication is controlled by whether the `API_KEY` environment variable is
set on the server (`src/middleware/auth.rs`, `auth_middleware`):

- **Not set**: every route (except a few always-public ones) accepts
  unauthenticated requests. This is the default in `docker-compose.yml` and
  most local setups.
- **Set**: every route except `/health`, `/healthz/*`, and `/unsubscribe`
  requires either `Authorization: Bearer <key>` or `X-Api-Key: <key>`.
- **Admin routes** (`/v1/admin/*`) are gated by a *separate*
  `ADMIN_API_KEY`, checked by a second middleware layer
  (`admin_auth_middleware`) independent of the regular key.
- **Multi-tenant mode**: when enabled, the resolved API key is hashed
  (SHA-256) and looked up against a `tenant_id` mapping; a valid key with no
  tenant mapping is rejected with `403`.

Full request/response examples and the status-code table (401 vs 403, wrong
key vs missing key) are in
[`docs/api-usage.md#authentication`](api-usage.md#authentication) — this
section only summarizes the mechanism.

## Testing against the live Swagger UI

`GET /docs` serves a minimal Swagger UI page
(`handlers::swagger_ui`) pointed at `GET /openapi.json`
(`handlers::openapi_json`, which serves the spec generated live from
`ApiDoc::openapi()` — see below). `/docs` gets a relaxed
Content-Security-Policy (`src/middleware/security_headers.rs`) specifically
to allow loading the `swagger-ui-dist` assets from `unpkg.com`.

**Important limitation, verified against the current handler
(`src/handlers.rs::swagger_ui`) and the `ApiDoc` definition
(`src/routes.rs`)**: the Swagger UI is initialized with only `url` and
`dom_id` — no `securitySchemes` are declared on the OpenAPI spec, and no
`requestInterceptor` is wired into the `SwaggerUIBundle` call. That means
there is **no "Authorize" button** in the current Swagger UI, and no way to
attach an API key to "Try it out" requests from the UI itself.

Practical ways to test authenticated endpoints today:

- **Easiest**: run against an instance where `API_KEY` is unset (e.g. plain
  `docker compose up`, which doesn't set it) — every "Try it out" call in
  Swagger UI will succeed with no auth needed.
- **Against a server that does enforce `API_KEY`**: "Try it out" requests
  from `/docs` will get `401`/`403` for protected routes. Either test those
  routes with `curl -H "X-Api-Key: $API_KEY"` instead (see
  [`docs/api-usage.md`](api-usage.md#authentication)), or use a browser
  extension that injects the header on requests to your host, since the
  Swagger UI page itself has no field for it.
- **Where to get a test key**: there's no key-issuance endpoint or portal —
  a "test key" is whatever value you set `API_KEY` (and `ADMIN_API_KEY`) to
  when starting your own instance (env var, `config.toml`, or
  `docker-compose.yml` override). There's no shared/public test key for a
  hosted instance.
- **Follow-up worth filing**: adding an `apiKey` `securitySchemes` entry to
  the `ApiDoc` derive plus enabling `persistAuthorization` in the
  `SwaggerUIBundle` call would give `/docs` a real Authorize button. That's a
  source change, out of scope for this doc.

## Versioning

Stable endpoints live under `/v1/`. Unversioned legacy paths still work but
return a `Deprecation: true` header and will be removed. Full policy —
what counts as a breaking change, deprecation lifecycle, how a `/v2/` would
be introduced — is in [`docs/api-versioning.md`](api-versioning.md). The
process for *recording* API-visible changes as they happen (not the policy
itself) is in [`docs/api-changelog.md`](api-changelog.md).

## How the OpenAPI spec is produced

`GET /openapi.json` is generated **at request time** from
`ApiDoc::openapi()` (`src/routes.rs`), which is built from `#[utoipa::path]`
annotations on the handler functions listed in the `ApiDoc` derive's
`paths(...)` — currently around 200 handlers. `cargo run --bin gen_openapi`
(wrapped by `make gen-openapi`) dumps that same live spec to a file and
copies it to `docs/openapi.json`:

```bash
make gen-openapi   # writes openapi.json, copies to docs/openapi.json
```

The **checked-in** `openapi.json` at the repo root currently documents only
13 of those routes (`/health`, `/status`, the core `/v1/events*`,
`/v1/contracts*`, and a couple of `/v1/admin/*` endpoints) — it was
hand-curated rather than freshly generated, and this doc round added
`example` values to it. If you regenerate it with `make gen-openapi` you'll
get the full ~200-route spec with no examples (utoipa annotations in
`src/handlers.rs` don't currently declare `example = "..."` values), so the
example coverage added here will be lost on the next regen unless it's
ported into the source annotations. See
[`docs/client-libraries.md`](client-libraries.md#regenerating-the-sdks) for
how this also affects SDK generation.

## Where to go next

- **REST recipes and full parameter reference**: [`docs/api-usage.md`](api-usage.md)
- **GraphQL**: [`docs/graphql_api.md`](graphql_api.md) (see status caveat above)
- **Client SDKs (Go/Python/TypeScript)**: [`docs/client-libraries.md`](client-libraries.md), [`docs/sdk-integration-guide.md`](sdk-integration-guide.md)
- **Local sandbox / test data**: [`docs/api-sandbox.md`](api-sandbox.md)
- **Worked examples (subscriptions, pagination, SSE, webhooks, batch lookups)**: [`docs/api-cookbook.md`](api-cookbook.md)
- **Versioning policy**: [`docs/api-versioning.md`](api-versioning.md)
- **Recording API changes**: [`docs/api-changelog.md`](api-changelog.md)
- **Webhook signing/verification**: [`docs/webhook_signing.md`](webhook_signing.md)
- **Postman collection generation**: [`docs/postman.md`](postman.md)
