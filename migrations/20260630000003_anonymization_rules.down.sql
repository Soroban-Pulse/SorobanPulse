-- Down migration for 20260630000003_anonymization_rules.sql
DROP INDEX IF EXISTS idx_anonymization_rules_enabled;
DROP TABLE IF EXISTS anonymization_rules CASCADE;
