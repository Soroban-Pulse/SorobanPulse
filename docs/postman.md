# Postman Collection

A Postman Collection v2.1 and three environment files are generated from the
Soroban Pulse OpenAPI spec (`openapi.json`) in CI and published as a release
artifact. You do not need to run the generator yourself — download the
artifact from the latest release and import it into Postman.

## Downloading the collection

1. Open the [latest release](https://github.com/Soroban-Pulse/soroban-pulse/releases/latest).
2. Under **Assets**, download `postman-collection.zip`.
3. Unzip it; the archive contains the collection and the environment files:

```
postman/
├── Soroban_Pulse.postman_collection.json   # Import this into Postman
└── environments/
    ├── local.postman_environment.json      # http://localhost:3000
    ├── testnet.postman_environment.json    # https://api.testnet.sorobanpulse.io
    └── mainnet.postman_environment.json    # https://api.sorobanpulse.io
```

The artifact is regenerated from `openapi.json` on every release, so it always
matches the published API reference.

## Importing into Postman

1. Open Postman → **Import** → drag `Soroban_Pulse.postman_collection.json`.
2. Import the environment file that matches your target:
   **Environments** → **Import** → choose `local.postman_environment.json`.
3. Select the environment from the top-right dropdown.
4. Fill in `api_key` (and `admin_api_key` for admin routes) in the
   environment's **Current Value** column.

## Authentication

| Endpoint pattern | Auth preset             |
|------------------|-------------------------|
| `/admin/*`       | `x-api-key: {{admin_api_key}}` |
| `/health`, `/docs`, `/metrics`, `/unsubscribe`, email tracking links | No auth |
| Everything else  | `x-api-key: {{api_key}}` (collection-level default) |

## Environment variables

| Variable         | Description                          |
|------------------|--------------------------------------|
| `base_url`       | API base URL (set per-environment)   |
| `api_key`        | Standard API key                     |
| `admin_api_key`  | Admin API key                        |
| `contract_id`    | Stellar contract address for filters |
| `tx_hash`        | Transaction hash for lookups         |
| `subscription_id`| Subscription UUID                    |

## Collection structure

Requests are grouped into folders that match the OpenAPI tags:

- **Health & System** — `/health`, `/healthz/*`, `/metrics`
- **Events** — event queries, SSE streams, contract-specific events
- **Contracts** — contract listing, metadata
- **Transactions** — transaction lookup
- **Subscriptions** — CRUD + webhook management
- **Admin** — replay, maintenance, rate-limit management
- **Docs** — OpenAPI spec, documentation routes

## Request bodies

Example request bodies are generated automatically from the OpenAPI schema.
Primitive fields use sensible defaults (`"string"`, `0`, `false`); objects
are expanded recursively up to four levels deep. Update the values in Postman
before sending.

## Regenerating locally (maintainers)

CI runs the generator automatically, but maintainers can reproduce the
artifact locally from the committed spec:

```bash
# Regenerate collection + environments in one shot
make gen-postman

# Or pipe the spec manually
cargo run --features dev-tools --bin gen_openapi | cargo run --features dev-tools --bin gen_postman

# From a saved spec file
cargo run --features dev-tools --bin gen_postman -- --input openapi.json

# Custom output directory
cargo run --features dev-tools --bin gen_postman -- --input openapi.json --output-dir postman/

# Dry-run: print the collection JSON to stdout
cargo run --features dev-tools --bin gen_postman -- --dry-run < openapi.json
```
