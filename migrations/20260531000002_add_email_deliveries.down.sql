-- Down migration for 20260531000002_add_email_deliveries.sql
DROP INDEX IF EXISTS idx_email_deliveries_recipient;
DROP INDEX IF EXISTS idx_email_deliveries_ab_template;
DROP INDEX IF EXISTS idx_email_deliveries_delivered_at;
DROP TABLE IF EXISTS email_deliveries CASCADE;
