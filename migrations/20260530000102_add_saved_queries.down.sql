-- Down migration for 20260530000102_add_saved_queries.sql
DROP INDEX IF EXISTS idx_saved_queries_name;
DROP INDEX IF EXISTS idx_saved_queries_created_by;
DROP INDEX IF EXISTS idx_saved_queries_created_at;
DROP TABLE IF EXISTS saved_queries CASCADE;
