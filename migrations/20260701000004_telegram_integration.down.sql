-- Down migration for 20260701000004_telegram_integration.sql
DROP INDEX IF EXISTS idx_telegram_integrations_subscription;
DROP INDEX IF EXISTS idx_telegram_messages_integration;
DROP INDEX IF EXISTS idx_telegram_messages_event;
DROP INDEX IF EXISTS idx_telegram_user_subscriptions_integration;
DROP TABLE IF EXISTS telegram_integrations CASCADE;
DROP TABLE IF EXISTS telegram_messages CASCADE;
DROP TABLE IF EXISTS telegram_user_subscriptions CASCADE;
