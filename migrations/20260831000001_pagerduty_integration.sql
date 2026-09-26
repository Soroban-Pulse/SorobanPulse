-- PagerDuty integration tables (Issue #951)

-- Per-subscription PagerDuty configuration
CREATE TABLE IF NOT EXISTS pagerduty_integrations (
    id                          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    subscription_id             UUID NOT NULL REFERENCES subscriptions(id) ON DELETE CASCADE,
    routing_key                 TEXT NOT NULL,
    service_name                VARCHAR(255) NOT NULL DEFAULT 'Soroban Pulse',
    -- PagerDuty REST API key for schedule/oncall lookups (optional)
    api_key                     TEXT,
    -- Optional escalation policy ID to attach when creating incidents
    escalation_policy_id        TEXT,
    -- Comma-separated list of contract IDs that should trigger incidents (empty = all)
    contract_filter             TEXT[] NOT NULL DEFAULT '{}',
    -- Comma-separated event types that trigger incidents (empty = all)
    event_type_filter           TEXT[] NOT NULL DEFAULT '{}',
    -- JSON object mapping event_type -> PagerDuty severity level
    severity_mapping            JSONB NOT NULL DEFAULT '{"contract":"error","diagnostic":"warning","system":"info"}',
    -- Whether the service should auto-resolve stale incidents
    auto_resolve                BOOLEAN NOT NULL DEFAULT TRUE,
    -- Minutes before an open incident without new events is auto-resolved
    auto_resolve_threshold_min  INTEGER NOT NULL DEFAULT 30,
    created_at                  TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at                  TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (subscription_id)
);

-- Tracks every triggered / acknowledged / resolved incident
CREATE TABLE IF NOT EXISTS pagerduty_incidents (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Correlates back to subscription configuration
    integration_id  UUID REFERENCES pagerduty_integrations(id) ON DELETE SET NULL,
    -- Deduplication key sent to PagerDuty Events API
    dedup_key       TEXT NOT NULL,
    -- PagerDuty-returned incident key (populated on first successful delivery)
    incident_key    TEXT,
    contract_id     TEXT NOT NULL,
    event_type      TEXT NOT NULL,
    -- 'triggered' | 'acknowledged' | 'resolved'
    status          TEXT NOT NULL DEFAULT 'triggered',
    -- When the incident was last acknowledged via our API
    acknowledged_at TIMESTAMP WITH TIME ZONE,
    acknowledged_by TEXT,
    -- When the incident was resolved (manually or auto)
    resolved_at     TIMESTAMP WITH TIME ZONE,
    created_at      TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at      TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (dedup_key)
);

-- Escalation policy cache — refreshed lazily from the PD REST API
CREATE TABLE IF NOT EXISTS pagerduty_escalation_policies (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    integration_id    UUID NOT NULL REFERENCES pagerduty_integrations(id) ON DELETE CASCADE,
    policy_id         TEXT NOT NULL,
    policy_name       TEXT NOT NULL,
    -- Full policy JSON from the PagerDuty API
    policy_json       JSONB,
    fetched_at        TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (integration_id, policy_id)
);

-- Indexes
CREATE INDEX idx_pd_integrations_subscription ON pagerduty_integrations(subscription_id);
CREATE INDEX idx_pd_incidents_integration      ON pagerduty_incidents(integration_id);
CREATE INDEX idx_pd_incidents_contract         ON pagerduty_incidents(contract_id);
CREATE INDEX idx_pd_incidents_status           ON pagerduty_incidents(status);
CREATE INDEX idx_pd_incidents_dedup_key        ON pagerduty_incidents(dedup_key);
CREATE INDEX idx_pd_escalation_integration     ON pagerduty_escalation_policies(integration_id);
