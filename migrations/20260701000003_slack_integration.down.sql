-- Down migration for 20260701000003_slack_integration.sql
DROP INDEX IF EXISTS idx_slack_integrations_subscription;
DROP INDEX IF EXISTS idx_slack_messages_integration;
DROP INDEX IF EXISTS idx_slack_messages_event;
DROP INDEX IF EXISTS idx_slack_user_subscriptions_integration;
DROP TABLE IF EXISTS slack_integrations CASCADE;
DROP TABLE IF EXISTS slack_messages CASCADE;
DROP TABLE IF EXISTS slack_user_subscriptions CASCADE;
