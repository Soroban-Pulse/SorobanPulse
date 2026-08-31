-- Rollback audit trail compliance enhancements
DROP VIEW IF EXISTS v_audit_retention_summary;
DROP FUNCTION IF EXISTS compute_audit_chain_hash(UUID);
DROP TABLE IF EXISTS audit_log_exports;
DROP TABLE IF EXISTS compliance_report_runs;
DROP INDEX IF EXISTS idx_audit_logs_signed_at;
DROP INDEX IF EXISTS idx_audit_logs_retention_class;
DROP INDEX IF EXISTS idx_audit_logs_compliance_tags;
ALTER TABLE audit_logs
  DROP COLUMN IF EXISTS log_hash,
  DROP COLUMN IF EXISTS chain_hash,
  DROP COLUMN IF EXISTS signed_at,
  DROP COLUMN IF EXISTS compliance_tags,
  DROP COLUMN IF EXISTS retention_class,
  DROP COLUMN IF EXISTS archived_at,
  DROP COLUMN IF EXISTS export_ref;
