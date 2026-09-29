-- Migration: Partition management support functions and views
-- Supports dynamic partition creation, pruning analysis, and monitoring

-- Enable partition pruning globally for this session
SET constraint_exclusion = partition;
SET enable_partition_pruning = on;

-- -------------------------------------------------------------------------
-- View: v_partition_overview
-- Shows all event partitions with size and row count information.
-- -------------------------------------------------------------------------
CREATE OR REPLACE VIEW v_partition_overview AS
SELECT
    c.relname                                           AS partition_name,
    pg_total_relation_size(c.oid)                       AS size_bytes,
    ROUND(pg_total_relation_size(c.oid) / 1024.0 / 1024.0, 2) AS size_mb,
    COALESCE(s.n_live_tup, 0)                           AS live_rows,
    COALESCE(s.n_dead_tup, 0)                           AS dead_rows,
    s.last_vacuum,
    s.last_autovacuum,
    s.last_analyze,
    s.last_autoanalyze,
    COALESCE(s.seq_scan, 0)                             AS seq_scan_count,
    COALESCE(s.idx_scan, 0)                             AS idx_scan_count
FROM pg_inherits i
JOIN pg_class c ON c.oid = i.inhrelid
JOIN pg_class p ON p.oid = i.inhparent
LEFT JOIN pg_stat_user_tables s ON s.relname = c.relname
WHERE p.relname = 'events'
ORDER BY c.relname;

-- -------------------------------------------------------------------------
-- Function: get_partition_size_stats()
-- Returns (partition_name, row_count, size_bytes) for each events partition.
-- -------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION get_partition_size_stats()
RETURNS TABLE(
    partition_name  TEXT,
    row_count       BIGINT,
    size_bytes      BIGINT,
    size_mb         NUMERIC
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        c.relname::TEXT,
        COALESCE(s.n_live_tup, 0)::BIGINT,
        pg_total_relation_size(c.oid)::BIGINT,
        ROUND(pg_total_relation_size(c.oid) / 1024.0 / 1024.0, 2)
    FROM pg_inherits i
    JOIN pg_class c ON c.oid = i.inhrelid
    JOIN pg_class p ON p.oid = i.inhparent
    LEFT JOIN pg_stat_user_tables s ON s.relname = c.relname
    WHERE p.relname = 'events'
    ORDER BY c.relname;
END;
$$ LANGUAGE plpgsql;

-- -------------------------------------------------------------------------
-- Function: get_partition_pruning_stats(from_ts, to_ts)
-- Identifies which partitions overlap a given timestamp range and reports
-- pruning effectiveness.
-- -------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION get_partition_pruning_stats(
    from_ts TIMESTAMPTZ,
    to_ts   TIMESTAMPTZ
)
RETURNS TABLE(
    partition_name        TEXT,
    partition_start       TIMESTAMPTZ,
    partition_end         TIMESTAMPTZ,
    would_be_accessed     BOOLEAN,
    row_count             BIGINT,
    size_bytes            BIGINT
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        c.relname::TEXT,
        -- Extract partition bounds from pg_get_expr
        to_timestamp(
            split_part(split_part(pg_get_expr(c.relpartbound, c.oid), '''', 2), '''', 1),
            'YYYY-MM-DD'
        ) AT TIME ZONE 'UTC',
        to_timestamp(
            split_part(split_part(pg_get_expr(c.relpartbound, c.oid), '''', 4), '''', 1),
            'YYYY-MM-DD'
        ) AT TIME ZONE 'UTC',
        (
            to_timestamp(split_part(split_part(pg_get_expr(c.relpartbound, c.oid), '''', 2), '''', 1), 'YYYY-MM-DD') AT TIME ZONE 'UTC' < to_ts
            AND
            to_timestamp(split_part(split_part(pg_get_expr(c.relpartbound, c.oid), '''', 4), '''', 1), 'YYYY-MM-DD') AT TIME ZONE 'UTC' > from_ts
        ),
        COALESCE(s.n_live_tup, 0)::BIGINT,
        pg_total_relation_size(c.oid)::BIGINT
    FROM pg_inherits i
    JOIN pg_class c ON c.oid = i.inhrelid
    JOIN pg_class p ON p.oid = i.inhparent
    LEFT JOIN pg_stat_user_tables s ON s.relname = c.relname
    WHERE p.relname = 'events'
    ORDER BY c.relname;
END;
$$ LANGUAGE plpgsql;

-- -------------------------------------------------------------------------
-- Function: auto_create_partitions(months_ahead INT)
-- Creates monthly event partitions for current + N future months.
-- Safe to call multiple times (IF NOT EXISTS).
-- -------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION auto_create_partitions(months_ahead INT DEFAULT 3)
RETURNS TABLE(partition_name TEXT, action TEXT) AS $$
DECLARE
    cur_date   DATE;
    target_dt  DATE;
    yr         INT;
    mo         INT;
    pname      TEXT;
    start_dt   DATE;
    end_dt     DATE;
BEGIN
    cur_date  := DATE_TRUNC('month', NOW())::DATE;
    target_dt := cur_date + (months_ahead || ' months')::INTERVAL;

    WHILE cur_date <= target_dt LOOP
        yr    := EXTRACT(YEAR  FROM cur_date)::INT;
        mo    := EXTRACT(MONTH FROM cur_date)::INT;
        pname := 'events_' || LPAD(yr::TEXT, 4, '0') || '_' || LPAD(mo::TEXT, 2, '0');
        start_dt := cur_date;
        end_dt   := cur_date + INTERVAL '1 month';

        -- Create partition (no-op if already exists)
        BEGIN
            EXECUTE format(
                'CREATE TABLE %I PARTITION OF events FOR VALUES FROM (%L) TO (%L)',
                pname, start_dt, end_dt
            );
            RETURN QUERY SELECT pname::TEXT, 'created'::TEXT;
        EXCEPTION WHEN duplicate_table THEN
            RETURN QUERY SELECT pname::TEXT, 'exists'::TEXT;
        END;

        cur_date := cur_date + INTERVAL '1 month';
    END LOOP;
END;
$$ LANGUAGE plpgsql;

-- -------------------------------------------------------------------------
-- Function: archive_old_partitions(archive_after_months INT, dry_run BOOL)
-- Renames partitions older than archive_after_months to archive_ prefix.
-- -------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION archive_old_partitions(
    archive_after_months INT DEFAULT 12,
    dry_run              BOOL DEFAULT TRUE
)
RETURNS TABLE(original_name TEXT, archive_name TEXT, action TEXT) AS $$
DECLARE
    cutoff DATE;
    rec    RECORD;
    aname  TEXT;
BEGIN
    cutoff := (NOW() - (archive_after_months || ' months')::INTERVAL)::DATE;

    FOR rec IN
        SELECT c.relname::TEXT AS tname
        FROM pg_inherits i
        JOIN pg_class c ON c.oid = i.inhrelid
        JOIN pg_class p ON p.oid = i.inhparent
        WHERE p.relname = 'events'
          AND c.relname ~ '^events_\d{4}_\d{2}$'
    LOOP
        -- Parse date from name
        IF TO_DATE(
            SUBSTRING(rec.tname FROM 'events_(\d{4}_\d{2})'),
            'YYYY_MM'
        ) < cutoff THEN
            aname := 'archive_' || rec.tname;
            IF dry_run THEN
                RETURN QUERY SELECT rec.tname::TEXT, aname::TEXT, 'dry_run'::TEXT;
            ELSE
                BEGIN
                    EXECUTE format('ALTER TABLE %I RENAME TO %I', rec.tname, aname);
                    RETURN QUERY SELECT rec.tname::TEXT, aname::TEXT, 'archived'::TEXT;
                EXCEPTION WHEN OTHERS THEN
                    RETURN QUERY SELECT rec.tname::TEXT, aname::TEXT,
                        format('error: %s', SQLERRM)::TEXT;
                END;
            END IF;
        END IF;
    END LOOP;
END;
$$ LANGUAGE plpgsql;

-- Run auto_create_partitions to ensure current and next 3 months exist
SELECT * FROM auto_create_partitions(3);
