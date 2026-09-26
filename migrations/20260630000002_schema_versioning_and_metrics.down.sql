-- Down migration for 20260630000002_schema_versioning_and_metrics.sql
DROP INDEX IF EXISTS idx_contract_schemas_version;
DROP TRIGGER IF EXISTS trg_schema_version ON contract_schemas;
DROP FUNCTION IF EXISTS increment_schema_version() CASCADE;
DROP TABLE IF EXISTS schema_validation_metrics CASCADE;
ALTER TABLE contract_schemas DROP COLUMN IF EXISTS version;
ALTER TABLE contract_schemas DROP COLUMN IF EXISTS description;
