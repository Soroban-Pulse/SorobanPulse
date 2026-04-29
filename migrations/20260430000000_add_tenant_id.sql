-- Multi-tenant support: optional tenant_id column on events.
-- NULL means the event belongs to the default (single-tenant) deployment.
-- In multi-tenant mode every row is stamped with the owning tenant's ID.
ALTER TABLE events ADD COLUMN IF NOT EXISTS tenant_id TEXT;

-- Partial index: only covers rows that actually have a tenant_id, keeping the
-- index small for single-tenant deployments where the column is always NULL.
CREATE INDEX IF NOT EXISTS idx_events_tenant_id
    ON events (tenant_id)
    WHERE tenant_id IS NOT NULL;

-- Composite index for the most common multi-tenant query pattern:
-- WHERE tenant_id = $1 ORDER BY ledger DESC, id DESC
CREATE INDEX IF NOT EXISTS idx_events_tenant_ledger_id
    ON events (tenant_id, ledger DESC, id DESC)
    WHERE tenant_id IS NOT NULL;

-- Tenant → API key mapping table.
-- We store a SHA-256 hex digest of the raw key so the plaintext never rests in
-- the database.  tenant_id is a free-form label chosen by the operator.
CREATE TABLE IF NOT EXISTS api_key_tenants (
    key_hash  TEXT PRIMARY KEY,   -- SHA-256(raw_api_key) hex-encoded
    tenant_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_api_key_tenants_tenant
    ON api_key_tenants (tenant_id);
