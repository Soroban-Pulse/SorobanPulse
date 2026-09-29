-- Issue #678: Add webhook templating system

-- Add webhook_template column to subscriptions
ALTER TABLE subscriptions
    ADD COLUMN IF NOT EXISTS webhook_template TEXT,
    ADD COLUMN IF NOT EXISTS webhook_template_enabled BOOLEAN NOT NULL DEFAULT false;

-- Create webhook_templates table for storing and managing templates
CREATE TABLE IF NOT EXISTS webhook_templates (
    id                UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    subscription_id   UUID        NOT NULL REFERENCES subscriptions(id) ON DELETE CASCADE,
    name              TEXT        NOT NULL,
    template_content  TEXT        NOT NULL,
    description       TEXT,
    is_active         BOOLEAN     NOT NULL DEFAULT true,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(subscription_id, name)
);

-- Create template_validation_log table for tracking template validation errors
CREATE TABLE IF NOT EXISTS template_validation_log (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    webhook_template_id UUID        REFERENCES webhook_templates(id) ON DELETE CASCADE,
    event_id            UUID,
    error_message       TEXT        NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create template_usage_stats table for tracking template usage
CREATE TABLE IF NOT EXISTS template_usage_stats (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    webhook_template_id UUID        NOT NULL REFERENCES webhook_templates(id) ON DELETE CASCADE,
    total_transforms    BIGINT      NOT NULL DEFAULT 0,
    successful          BIGINT      NOT NULL DEFAULT 0,
    failed              BIGINT      NOT NULL DEFAULT 0,
    last_used_at        TIMESTAMPTZ,
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Add indexes for better query performance
CREATE INDEX IF NOT EXISTS idx_webhook_templates_subscription
    ON webhook_templates(subscription_id, is_active);
CREATE INDEX IF NOT EXISTS idx_template_validation_log_template
    ON template_validation_log(webhook_template_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_template_usage_stats_template
    ON template_usage_stats(webhook_template_id);
