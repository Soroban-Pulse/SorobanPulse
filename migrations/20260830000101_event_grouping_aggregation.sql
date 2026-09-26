-- Issue #934: Event grouping and aggregation enhancements
-- Extends aggregation tables with group metrics and statistics tracking

-- Group metrics table: stores computed statistics per aggregation group
CREATE TABLE IF NOT EXISTS group_metrics (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    rule_id             UUID        NOT NULL REFERENCES aggregation_rules(id) ON DELETE CASCADE,
    subscription_id     UUID        NOT NULL REFERENCES subscriptions(id) ON DELETE CASCADE,
    group_key           TEXT        NOT NULL,    -- Serialised group-by values (e.g. "contract_id=CA...")
    window_start        TIMESTAMPTZ NOT NULL,
    window_end          TIMESTAMPTZ NOT NULL,
    event_count         BIGINT      NOT NULL DEFAULT 0,
    avg_value           DOUBLE PRECISION,
    min_value           DOUBLE PRECISION,
    max_value           DOUBLE PRECISION,
    sum_value           DOUBLE PRECISION,
    distinct_count      BIGINT,
    extra_metrics       JSONB,                   -- Any additional computed metrics
    computed_at         TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Efficient lookup by rule + window + group
CREATE INDEX IF NOT EXISTS idx_group_metrics_rule_window
    ON group_metrics(rule_id, window_start DESC, window_end);

-- Lookup by subscription for cross-rule dashboards
CREATE INDEX IF NOT EXISTS idx_group_metrics_subscription
    ON group_metrics(subscription_id, computed_at DESC);

-- Lookup by group key for trend/comparison queries
CREATE INDEX IF NOT EXISTS idx_group_metrics_group_key
    ON group_metrics(group_key, window_start DESC);

-- Composite index used by get_group_statistics (rule + group + time range)
CREATE INDEX IF NOT EXISTS idx_group_metrics_rule_group_time
    ON group_metrics(rule_id, group_key, window_start DESC);

-- Add aggregation_ops column to aggregation_rules to persist per-rule operation config
ALTER TABLE aggregation_rules
    ADD COLUMN IF NOT EXISTS aggregation_ops JSONB;

-- Add batch_size column so the AggregationOptimizer can record its batch setting
ALTER TABLE aggregation_rules
    ADD COLUMN IF NOT EXISTS batch_size INT NOT NULL DEFAULT 100;
