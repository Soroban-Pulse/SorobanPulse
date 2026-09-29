-- Down migration for 20260530000001_add_geospatial_support.sql
DROP INDEX IF EXISTS idx_events_location;
DROP TRIGGER IF EXISTS trigger_extract_coordinates ON events;
DROP FUNCTION IF EXISTS extract_coordinates_from_event_data() CASCADE;
ALTER TABLE events DROP COLUMN IF EXISTS latitude;
ALTER TABLE events DROP COLUMN IF EXISTS longitude;
