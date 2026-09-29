-- Contract labels and metadata registry (#1066)
CREATE TABLE IF NOT EXISTS contract_metadata (
    contract_id   TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    description   TEXT,
    project_url   TEXT,
    source_repo   TEXT,
    tags          TEXT[] NOT NULL DEFAULT '{}',
    verified      BOOLEAN NOT NULL DEFAULT FALSE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_contract_metadata_tags ON contract_metadata USING GIN (tags);
CREATE INDEX IF NOT EXISTS idx_contract_metadata_verified ON contract_metadata (verified);
