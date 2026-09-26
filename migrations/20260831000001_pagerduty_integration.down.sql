-- Down migration: remove PagerDuty tables (Issue #951)
DROP TABLE IF EXISTS pagerduty_escalation_policies;
DROP TABLE IF EXISTS pagerduty_incidents;
DROP TABLE IF EXISTS pagerduty_integrations;
