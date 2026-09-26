-- Down migration for 20260531000003_add_suppression_lists.sql
DROP INDEX IF EXISTS idx_suppression_lists_target;
DROP INDEX IF EXISTS idx_suppression_lists_type;
DROP INDEX IF EXISTS idx_suppression_lists_expires;
DROP TABLE IF EXISTS suppression_lists CASCADE;
