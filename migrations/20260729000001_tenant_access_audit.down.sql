-- Rollback: drop tenant audit log and quota tables (issue #812).
DROP TABLE IF EXISTS tenant_access_audit CASCADE;
DROP TABLE IF EXISTS tenant_quotas CASCADE;
DROP FUNCTION IF EXISTS touch_tenant_quotas_updated_at CASCADE;
