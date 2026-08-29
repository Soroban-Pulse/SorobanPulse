-- Issue #884: Add subscription pause/resume with state preservation

-- Add pause state tracking to subscriptions table
ALTER TABLE subscriptions ADD COLUMN IF NOT EXISTS pause_until TIMESTAMPTZ;
ALTER TABLE subscriptions ADD COLUMN IF NOT EXISTS pause_reason TEXT;
ALTER TABLE subscriptions ADD COLUMN IF NOT EXISTS paused_at TIMESTAMPTZ;

-- Create subscription_pause_resume_log table for audit trail
CREATE TABLE IF NOT EXISTS subscription_pause_resume_log (
    id                UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    subscription_id   UUID        NOT NULL REFERENCES subscriptions(id) ON DELETE CASCADE,
    action            TEXT        NOT NULL,  -- 'paused' or 'resumed'
    reason            TEXT,
    paused_until      TIMESTAMPTZ,           -- Only for pause actions
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by        TEXT                   -- Optional: API key or admin identifier
);

-- Create index for efficient pause/resume lookups
CREATE INDEX IF NOT EXISTS idx_subscriptions_pause_until
    ON subscriptions(pause_until) WHERE pause_until IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_subscription_pause_resume_log_subscription
    ON subscription_pause_resume_log(subscription_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_subscription_pause_resume_log_action
    ON subscription_pause_resume_log(subscription_id, action, created_at DESC);
