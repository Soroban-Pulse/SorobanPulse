-- Down migration for 20260701000001_github_integration.sql
DROP INDEX IF EXISTS idx_github_integrations_subscription;
DROP INDEX IF EXISTS idx_github_issues_integration;
DROP INDEX IF EXISTS idx_github_issues_event;
DROP TABLE IF EXISTS github_integrations CASCADE;
DROP TABLE IF EXISTS github_issues CASCADE;
