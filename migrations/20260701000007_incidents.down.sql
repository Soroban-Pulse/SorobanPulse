-- Reverts 20260701000007_incidents.sql
DROP INDEX IF EXISTS idx_incidents_status_opened_at;
DROP INDEX IF EXISTS idx_incidents_kind_opened_at;
DROP TABLE IF EXISTS incidents;
