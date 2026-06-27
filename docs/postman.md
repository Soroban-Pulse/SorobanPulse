# Postman Collection Generator

Generates a Postman Collection v2.1 and three environment files from the
Soroban Pulse OpenAPI spec.

## Quick start

```bash
# Regenerate collection + environments in one shot
make gen-postman

# Or pipe the spec manually
cargo run --bin gen_openapi | cargo run --bin gen_postman

# From a saved spec file
cargo run --bin gen_postman -- --input openapi.json

# Custom output directory
cargo run --bin gen_postman -- --input openapi.json --output-dir postman/

# Dry-run: print the collection JSON to stdout
cargo run --bin gen_postman -- --dry-run < openapi.json
```

## Output

```
postman/
├── Soroban_Pulse.postman_collection.json   # Import this into Postman
└── environments/
    ├── local.postman_environment.json      # http://localhost:3000
    ├── testnet.postman_environment.json    # https://api.testnet.sorobanpulse.io
    └── mainnet.postman_environment.json    # https://api.sorobanpulse.io
```

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
