-- no-transaction
-- CREATE INDEX CONCURRENTLY cannot run inside a transaction block.
-- SQLx skips the implicit transaction wrapper when a migration starts with "-- no-transaction".
-- IF NOT EXISTS makes this idempotent; a DO/EXCEPTION block is unnecessary and would
-- reintroduce a transaction context, which is incompatible with CONCURRENTLY.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_events_event_data_gin
    ON events USING GIN (event_data jsonb_path_ops);
