DROP TABLE IF EXISTS api_key_tenants;
DROP INDEX IF EXISTS idx_events_tenant_ledger_id;
DROP INDEX IF EXISTS idx_events_tenant_id;
ALTER TABLE events DROP COLUMN IF EXISTS tenant_id;
