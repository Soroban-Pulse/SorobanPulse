-- Issue #693: Implement database statistics auto-analysis
-- This migration sets up automatic statistics analysis and refresh mechanisms

-- Step 1: Create a table to track statistics metadata
CREATE TABLE IF NOT EXISTS table_statistics_metadata (
    table_name TEXT PRIMARY KEY,
    last_analyzed TIMESTAMPTZ,
    last_vacuumed TIMESTAMPTZ,
    row_count BIGINT,
    live_row_count BIGINT,
    dead_row_count BIGINT,
    table_size_bytes BIGINT,
    is_stale BOOLEAN DEFAULT TRUE,
    staleness_threshold_hours INT DEFAULT 24,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Step 2: Create indexes on statistics metadata table
CREATE INDEX IF NOT EXISTS idx_table_statistics_is_stale
    ON table_statistics_metadata(is_stale);

CREATE INDEX IF NOT EXISTS idx_table_statistics_last_analyzed
    ON table_statistics_metadata(last_analyzed DESC);

-- Step 3: Create a table to track ANALYZE job history
CREATE TABLE IF NOT EXISTS statistics_analysis_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    table_name TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    duration_seconds INT,
    row_count_analyzed BIGINT,
    status TEXT NOT NULL DEFAULT 'running',
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Step 4: Create indexes on job history
CREATE INDEX IF NOT EXISTS idx_statistics_jobs_table_name
    ON statistics_analysis_jobs(table_name);

CREATE INDEX IF NOT EXISTS idx_statistics_jobs_status
    ON statistics_analysis_jobs(status);

CREATE INDEX IF NOT EXISTS idx_statistics_jobs_started_at
    ON statistics_analysis_jobs(started_at DESC);

-- Step 5: Create function to detect stale statistics
CREATE OR REPLACE FUNCTION detect_stale_statistics()
RETURNS TABLE(table_name TEXT, is_stale BOOLEAN, hours_since_analyze INT, staleness_threshold_hours INT) AS $$
BEGIN
    RETURN QUERY
    SELECT
        tsm.table_name,
        (EXTRACT(HOUR FROM (NOW() - tsm.last_analyzed)) > tsm.staleness_threshold_hours) AS is_stale,
        COALESCE(EXTRACT(HOUR FROM (NOW() - tsm.last_analyzed))::INT, 9999) AS hours_since_analyze,
        tsm.staleness_threshold_hours
    FROM table_statistics_metadata tsm
    ORDER BY hours_since_analyze DESC;
END;
$$ LANGUAGE plpgsql;

-- Step 6: Create function to refresh table statistics
CREATE OR REPLACE FUNCTION refresh_table_statistics(target_table TEXT DEFAULT NULL)
RETURNS TABLE(table_name TEXT, status TEXT, duration_seconds INT) AS $$
DECLARE
    start_time TIMESTAMP;
    end_time TIMESTAMP;
    duration_int INT;
    current_table TEXT;
    row_count_val BIGINT;
    table_cursor CURSOR FOR
        SELECT tablename FROM pg_tables WHERE schemaname = 'public'
        AND (target_table IS NULL OR tablename = target_table)
        ORDER BY tablename;
    job_id UUID;
BEGIN
    OPEN table_cursor;
    LOOP
        FETCH table_cursor INTO current_table;
        EXIT WHEN current_table IS NULL;

        start_time := CLOCK_TIMESTAMP();
        job_id := gen_random_uuid();

        -- Insert job record
        INSERT INTO statistics_analysis_jobs (id, table_name, started_at, status)
        VALUES (job_id, current_table, start_time, 'running');

        -- Execute ANALYZE
        BEGIN
            EXECUTE format('ANALYZE %I', current_table);

            end_time := CLOCK_TIMESTAMP();
            duration_int := EXTRACT(EPOCH FROM (end_time - start_time))::INT;

            -- Get current row count
            EXECUTE format('SELECT count(*) FROM %I', current_table) INTO row_count_val;

            -- Update job record with success
            UPDATE statistics_analysis_jobs
            SET completed_at = end_time,
                duration_seconds = duration_int,
                row_count_analyzed = row_count_val,
                status = 'success'
            WHERE id = job_id;

            -- Update statistics metadata
            INSERT INTO table_statistics_metadata (table_name, last_analyzed, row_count, is_stale)
            VALUES (current_table, end_time, row_count_val, FALSE)
            ON CONFLICT (table_name) DO UPDATE
            SET last_analyzed = end_time,
                row_count = row_count_val,
                is_stale = FALSE,
                updated_at = NOW();

            RETURN QUERY SELECT current_table::TEXT, 'success'::TEXT, duration_int;

        EXCEPTION WHEN OTHERS THEN
            end_time := CLOCK_TIMESTAMP();
            duration_int := EXTRACT(EPOCH FROM (end_time - start_time))::INT;

            -- Update job record with error
            UPDATE statistics_analysis_jobs
            SET completed_at = end_time,
                duration_seconds = duration_int,
                status = 'error',
                error_message = SQLERRM
            WHERE id = job_id;

            RETURN QUERY SELECT current_table::TEXT, format('error: %s', SQLERRM)::TEXT, duration_int;
        END;
    END LOOP;
    CLOSE table_cursor;
END;
$$ LANGUAGE plpgsql;

-- Step 7: Create function to initialize statistics tracking for all tables
CREATE OR REPLACE FUNCTION initialize_statistics_tracking()
RETURNS void AS $$
DECLARE
    current_table TEXT;
BEGIN
    FOR current_table IN
        SELECT tablename FROM pg_tables WHERE schemaname = 'public'
    LOOP
        INSERT INTO table_statistics_metadata (table_name, last_analyzed, is_stale)
        VALUES (current_table, NOW(), FALSE)
        ON CONFLICT (table_name) DO NOTHING;
    END LOOP;

    RAISE NOTICE 'Statistics tracking initialized for all public tables';
END;
$$ LANGUAGE plpgsql;

-- Step 8: Create function to get statistics report
CREATE OR REPLACE FUNCTION get_statistics_report()
RETURNS TABLE(
    table_name TEXT,
    last_analyzed TIMESTAMPTZ,
    hours_since_analyze INT,
    is_stale BOOLEAN,
    row_count BIGINT,
    table_size_mb NUMERIC,
    recent_jobs_count INT
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        tsm.table_name,
        tsm.last_analyzed,
        COALESCE(EXTRACT(HOUR FROM (NOW() - tsm.last_analyzed))::INT, 9999),
        tsm.is_stale,
        tsm.row_count,
        ROUND((pg_total_relation_size(format('%I', tsm.table_name))::NUMERIC / 1024 / 1024), 2),
        COUNT(sj.id)::INT
    FROM table_statistics_metadata tsm
    LEFT JOIN statistics_analysis_jobs sj ON tsm.table_name = sj.table_name
        AND sj.started_at > NOW() - INTERVAL '7 days'
    GROUP BY tsm.table_name, tsm.last_analyzed, tsm.is_stale, tsm.row_count
    ORDER BY tsm.last_analyzed ASC NULLS LAST;
END;
$$ LANGUAGE plpgsql;

-- Step 9: Create trigger to mark statistics as stale when table is modified
CREATE OR REPLACE FUNCTION mark_statistics_stale()
RETURNS TRIGGER AS $$
BEGIN
    UPDATE table_statistics_metadata
    SET is_stale = TRUE,
        updated_at = NOW()
    WHERE table_name = TG_TABLE_NAME::TEXT;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Step 10: Create triggers on main tables for stale detection
CREATE TRIGGER mark_events_stats_stale
AFTER INSERT OR UPDATE OR DELETE ON events
FOR EACH ROW
EXECUTE FUNCTION mark_statistics_stale();

-- Step 11: Initialize tracking for all tables
SELECT initialize_statistics_tracking();

-- Step 12: Create initial analysis job for all tables
SELECT refresh_table_statistics();
