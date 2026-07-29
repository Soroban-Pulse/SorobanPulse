-- Issue #817: Adaptive pool tuning — runtime config snapshots and event log.
--
-- pool_tuning_snapshots: stores periodic snapshots from the adaptive tuner.
-- pool_config_history:   audit trail of runtime configuration changes.

CREATE TABLE IF NOT EXISTS pool_tuning_snapshots (
    id              BIGSERIAL PRIMARY KEY,
    recommended_min INTEGER     NOT NULL,
    recommended_max INTEGER     NOT NULL,
    avg_utilization DOUBLE PRECISION NOT NULL,
    scale_up_advised  BOOLEAN   NOT NULL DEFAULT false,
    scale_down_advised BOOLEAN  NOT NULL DEFAULT false,
    config_version  INTEGER     NOT NULL DEFAULT 1,
    recorded_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_pool_tuning_snapshots_recorded_at
    ON pool_tuning_snapshots (recorded_at DESC);

CREATE TABLE IF NOT EXISTS pool_config_history (
    id              BIGSERIAL PRIMARY KEY,
    config_version  INTEGER     NOT NULL,
    config_variant  TEXT        NOT NULL DEFAULT 'default',
    config_json     JSONB       NOT NULL,
    applied_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    applied_by      TEXT        NOT NULL DEFAULT 'system',
    rollback_of     INTEGER                          -- references a prior version
);

CREATE INDEX IF NOT EXISTS idx_pool_config_history_applied_at
    ON pool_config_history (applied_at DESC);
