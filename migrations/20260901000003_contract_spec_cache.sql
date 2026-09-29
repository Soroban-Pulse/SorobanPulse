-- Specs cached by WASM hash: one WASM can serve many contracts (issue #1057).
CREATE TABLE IF NOT EXISTS contract_spec_cache (
    wasm_hash TEXT PRIMARY KEY,
    spec JSONB NOT NULL,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
-- Contracts with no fetchable WASM (SAC) or failed lookups, to avoid refetching.
CREATE TABLE IF NOT EXISTS contract_spec_attempts (
    contract_id TEXT PRIMARY KEY,
    status TEXT NOT NULL,
    attempted_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
