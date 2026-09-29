-- Rollback: Issue #693 - Implement database statistics auto-analysis

-- Step 1: Drop triggers
DROP TRIGGER IF EXISTS mark_events_stats_stale ON events;

-- Step 2: Drop functions
DROP FUNCTION IF EXISTS mark_statistics_stale();
DROP FUNCTION IF EXISTS get_statistics_report();
DROP FUNCTION IF EXISTS initialize_statistics_tracking();
DROP FUNCTION IF EXISTS refresh_table_statistics(TEXT);
DROP FUNCTION IF EXISTS detect_stale_statistics();

-- Step 3: Drop tables
DROP TABLE IF EXISTS statistics_analysis_jobs;
DROP TABLE IF EXISTS table_statistics_metadata;
