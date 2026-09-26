-- Down migration for 20260428000004_matview_hourly_volume.sql
DROP INDEX IF EXISTS idx_events_hourly_volume_unique;
DROP MATERIALIZED VIEW IF EXISTS events_hourly_volume;
