-- Rollback multi-tenant support
DROP INDEX IF EXISTS idx_tenants_created_at;
DROP TABLE IF EXISTS tenants;

DROP INDEX IF EXISTS idx_tenant_rls_tenant_id;
DROP TABLE IF EXISTS tenant_rls_policies;

DROP INDEX IF EXISTS idx_events_tenant_timestamp;
DROP INDEX IF EXISTS idx_events_tenant_ledger;
DROP INDEX IF EXISTS idx_events_tenant_contract;
DROP INDEX IF EXISTS idx_events_tenant_id;

ALTER TABLE events DROP COLUMN IF EXISTS tenant_id;
