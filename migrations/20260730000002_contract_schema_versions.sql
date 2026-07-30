-- Issue #816: schema registry version history and compatibility tracking.
CREATE TABLE IF NOT EXISTS contract_schema_versions (
    contract_id TEXT NOT NULL,
    version INTEGER NOT NULL,
    schema JSONB NOT NULL,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (contract_id, version)
);

CREATE INDEX IF NOT EXISTS idx_contract_schema_versions_contract
    ON contract_schema_versions (contract_id, version DESC);
