-- Down migration for 20260627000101_feature_flags.sql
DROP INDEX IF EXISTS idx_feature_flag_audit_flag_id;
DROP INDEX IF EXISTS idx_feature_flag_audit_created_at;
DROP TABLE IF EXISTS feature_flags CASCADE;
DROP TABLE IF EXISTS feature_flag_audit CASCADE;
