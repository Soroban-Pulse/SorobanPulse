-- Down migration for 20260625000003_notification_templates.sql
DROP INDEX IF EXISTS idx_notification_templates_name;
DROP INDEX IF EXISTS idx_notification_templates_active;
DROP TABLE IF EXISTS notification_templates CASCADE;
