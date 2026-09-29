-- Down migration for 20260625000201_maintenance_windows.sql
DROP INDEX IF EXISTS idx_maintenance_windows_active;
DROP TABLE IF EXISTS maintenance_windows CASCADE;
