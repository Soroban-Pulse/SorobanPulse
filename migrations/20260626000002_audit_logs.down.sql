-- Down migration for 20260626000002_audit_logs.sql
DROP INDEX IF EXISTS idx_audit_logs_event_type;
DROP INDEX IF EXISTS idx_audit_logs_resource_type;
DROP INDEX IF EXISTS idx_audit_logs_resource_id;
DROP INDEX IF EXISTS idx_audit_logs_api_key_hash;
DROP INDEX IF EXISTS idx_audit_logs_user_email;
DROP INDEX IF EXISTS idx_audit_logs_created_at;
DROP INDEX IF EXISTS idx_audit_logs_ip_address;
DROP INDEX IF EXISTS idx_audit_logs_success;
DROP INDEX IF EXISTS idx_audit_logs_severity;
DROP INDEX IF EXISTS idx_audit_logs_event_created;
DROP INDEX IF EXISTS idx_audit_logs_resource_created;
DROP INDEX IF EXISTS idx_audit_logs_user_created;
DROP TRIGGER IF EXISTS trigger_set_audit_log_retention ON audit_logs;
DROP FUNCTION IF EXISTS set_audit_log_retention() CASCADE;
DROP FUNCTION IF EXISTS cleanup_expired_audit_logs() CASCADE;
DROP TABLE IF EXISTS audit_logs CASCADE;
