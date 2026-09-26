-- Down migration for 20260701000002_discord_integration.sql
DROP INDEX IF EXISTS idx_discord_integrations_subscription;
DROP INDEX IF EXISTS idx_discord_messages_integration;
DROP INDEX IF EXISTS idx_discord_messages_event;
DROP TABLE IF EXISTS discord_integrations CASCADE;
DROP TABLE IF EXISTS discord_messages CASCADE;
