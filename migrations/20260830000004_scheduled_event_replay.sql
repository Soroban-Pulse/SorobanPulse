-- Issue #930: Scheduled event replay.
--
-- Allows operators to define recurring schedules that automatically trigger
-- event replay operations. Supports interval-based, cron-based, and one-shot
-- schedules. Each execution is logged in scheduled_replay_runs for auditing.

CREATE TABLE IF NOT EXISTS replay_schedules (
    id                     UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name                   TEXT        NOT NULL,
    subscription_id        UUID        NOT NULL REFERENCES subscriptions(id) ON DELETE CASCADE,
    -- 'interval', 'cron', or 'one_shot'
    schedule_type          TEXT        NOT NULL DEFAULT 'interval',
    -- Cron expression (e.g. '0 */6 * * *') — used when schedule_type = 'cron'
    cron_expression        TEXT,
    -- Seconds between runs — used when schedule_type = 'interval'
    interval_secs          BIGINT,
    -- Optional contract ID filter applied during replay
    filter_contract_id     TEXT,
    -- Optional event type filter applied during replay
    filter_event_type      TEXT,
    -- Maximum number of events to replay per run (NULL = no limit)
    max_events             BIGINT,
    -- How far back (in seconds) to replay from at each run
    replay_from_offset_secs BIGINT     NOT NULL DEFAULT 3600,
    is_active              BOOLEAN     NOT NULL DEFAULT true,
    last_run_at            TIMESTAMPTZ,
    next_run_at            TIMESTAMPTZ,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Per-execution run log
CREATE TABLE IF NOT EXISTS scheduled_replay_runs (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    schedule_id     UUID        NOT NULL REFERENCES replay_schedules(id) ON DELETE CASCADE,
    -- References the underlying on-demand replay record, if one was created
    replay_id       UUID        REFERENCES replay_status(id),
    -- 'running', 'completed', 'failed'
    status          TEXT        NOT NULL DEFAULT 'running',
    events_replayed BIGINT      NOT NULL DEFAULT 0,
    error_message   TEXT,
    started_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at    TIMESTAMPTZ
);

-- Look up active schedules for a given subscription efficiently
CREATE INDEX IF NOT EXISTS idx_replay_schedules_subscription
    ON replay_schedules (subscription_id, is_active);

-- Scheduler poll: find schedules whose next_run_at has arrived
CREATE INDEX IF NOT EXISTS idx_replay_schedules_next_run
    ON replay_schedules (next_run_at, is_active)
    WHERE is_active = true;

-- Audit history for a specific schedule, newest first
CREATE INDEX IF NOT EXISTS idx_scheduled_replay_runs_schedule
    ON scheduled_replay_runs (schedule_id, started_at DESC);
