-- Down migration for 20260729000101_partition_management_functions.sql
DROP FUNCTION IF EXISTS get_partition_size_stats() CASCADE;
DROP FUNCTION IF EXISTS get_partition_pruning_stats(TIMESTAMPTZ,TIMESTAMPTZ) CASCADE;
DROP FUNCTION IF EXISTS auto_create_partitions(INT) CASCADE;
DROP FUNCTION IF EXISTS archive_old_partitions(INT,BOOL) CASCADE;
DROP VIEW IF EXISTS v_partition_overview;
