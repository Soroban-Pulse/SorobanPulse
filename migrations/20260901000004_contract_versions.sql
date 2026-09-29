CREATE TABLE IF NOT EXISTS contract_versions (
    id BIGSERIAL PRIMARY KEY,
    contract_id TEXT NOT NULL,
    wasm_hash TEXT NOT NULL,
    first_seen_ledger BIGINT,
    observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_upgrade BOOLEAN NOT NULL DEFAULT false
);
CREATE INDEX IF NOT EXISTS idx_contract_versions_contract ON contract_versions(contract_id, id DESC);
