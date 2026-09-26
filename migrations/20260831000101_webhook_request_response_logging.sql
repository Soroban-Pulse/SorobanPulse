-- Issue #937: Add webhook request/response logging

CREATE TABLE IF NOT EXISTS webhook_logs (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    url                 TEXT        NOT NULL,
    request_headers     JSONB,
    request_body        JSONB,
    request_truncated   BOOLEAN     NOT NULL DEFAULT FALSE,
    response_status     INTEGER,
    response_body       JSONB,
    response_truncated  BOOLEAN     NOT NULL DEFAULT FALSE,
    duration_ms         BIGINT,
    contract_id         TEXT,
    event_type          TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_webhook_logs_created_at
    ON webhook_logs(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_webhook_logs_url
    ON webhook_logs(url, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_webhook_logs_contract_id
    ON webhook_logs(contract_id, created_at DESC) WHERE contract_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_webhook_logs_response_status
    ON webhook_logs(response_status, created_at DESC) WHERE response_status IS NOT NULL;

-- Audit trail of who accessed webhook logs (access is sensitive: bodies may
-- contain partially-masked payload data).
CREATE TABLE IF NOT EXISTS webhook_log_access_audit (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    accessor        TEXT        NOT NULL,
    action          TEXT        NOT NULL, -- 'search', 'export'
    filter_summary  TEXT,
    result_count    INTEGER,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_webhook_log_access_audit_created_at
    ON webhook_log_access_audit(created_at DESC);
