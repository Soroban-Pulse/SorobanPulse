-- Issue #812: Tenant access audit log.
--
-- Records every tenant-scoped data access so operators can detect cross-tenant
-- probing, replay attacks, and unexpected access patterns.
--
-- Design decisions:
--   - Append-only: no UPDATE or DELETE is permitted via the application role.
--   - Partitioned by month (RANGE on accessed_at) so old partitions can be
--     detached and archived without locking the live table.
--   - Indexed on (tenant_id, accessed_at DESC) for the most common query:
--     "show me the last N accesses for tenant X".
--   - Indexed on (api_key_hash, accessed_at DESC) to answer:
--     "which tenant IDs has this key touched?" (anomaly detection).

CREATE TABLE IF NOT EXISTS tenant_access_audit (
    id             BIGSERIAL        PRIMARY KEY,

    -- Resolved tenant for this request (NULL = admin / unscoped).
    tenant_id      TEXT,

    -- SHA-256 hex digest of the raw API key presented (never plaintext).
    api_key_hash   TEXT             NOT NULL,

    -- HTTP method and path that triggered the access.
    http_method    TEXT             NOT NULL,
    http_path      TEXT             NOT NULL,

    -- Optional query string (truncated to 512 chars to avoid PII in long URLs).
    query_string   TEXT,

    -- Client IP address (after proxy header resolution).
    client_ip      INET,

    -- Outbound HTTP status returned to the client.
    response_status SMALLINT        NOT NULL,

    -- How long the handler took to complete (microseconds).
    duration_us    INTEGER,

    -- W3C trace-id for correlation with distributed traces (#813).
    trace_id       TEXT,

    -- Wall-clock timestamp of the access.
    accessed_at    TIMESTAMPTZ      NOT NULL DEFAULT NOW()
) PARTITION BY RANGE (accessed_at);

-- Create the first two monthly partitions (current month + next month).
-- The pruner / archiver job (src/pruner.rs) is responsible for creating future
-- partitions and detaching expired ones according to the retention policy.
DO $$
DECLARE
    this_month  DATE := DATE_TRUNC('month', NOW())::DATE;
    next_month  DATE := (DATE_TRUNC('month', NOW()) + INTERVAL '1 month')::DATE;
    after_next  DATE := (DATE_TRUNC('month', NOW()) + INTERVAL '2 months')::DATE;
    part_name   TEXT;
BEGIN
    -- Current month
    part_name := 'tenant_access_audit_' || TO_CHAR(this_month, 'YYYY_MM');
    IF NOT EXISTS (
        SELECT 1 FROM pg_class WHERE relname = part_name
    ) THEN
        EXECUTE FORMAT(
            'CREATE TABLE %I PARTITION OF tenant_access_audit
             FOR VALUES FROM (%L) TO (%L)',
            part_name, this_month, next_month
        );
    END IF;

    -- Next month
    part_name := 'tenant_access_audit_' || TO_CHAR(next_month, 'YYYY_MM');
    IF NOT EXISTS (
        SELECT 1 FROM pg_class WHERE relname = part_name
    ) THEN
        EXECUTE FORMAT(
            'CREATE TABLE %I PARTITION OF tenant_access_audit
             FOR VALUES FROM (%L) TO (%L)',
            part_name, next_month, after_next
        );
    END IF;
END $$;

-- Primary lookup index: recent accesses for a given tenant.
CREATE INDEX IF NOT EXISTS idx_tenant_audit_tenant_time
    ON tenant_access_audit (tenant_id, accessed_at DESC)
    WHERE tenant_id IS NOT NULL;

-- Secondary index: accesses by key hash (key-rotation / anomaly detection).
CREATE INDEX IF NOT EXISTS idx_tenant_audit_key_time
    ON tenant_access_audit (api_key_hash, accessed_at DESC);

-- Fast lookup by trace ID for cross-correlation with distributed traces.
CREATE INDEX IF NOT EXISTS idx_tenant_audit_trace_id
    ON tenant_access_audit (trace_id)
    WHERE trace_id IS NOT NULL;

-- Partial index for failed requests (response_status >= 400) to power the
-- security dashboard's "access denied" view without scanning all rows.
CREATE INDEX IF NOT EXISTS idx_tenant_audit_failures
    ON tenant_access_audit (tenant_id, accessed_at DESC)
    WHERE response_status >= 400;

-- ─────────────────────────────────────────────────────────────────────────────
-- Tenant quota / capacity tracking table
-- ─────────────────────────────────────────────────────────────────────────────
-- Stores configurable per-tenant request quotas and current consumption
-- counters (reset hourly by the background pruner).

CREATE TABLE IF NOT EXISTS tenant_quotas (
    tenant_id              TEXT        PRIMARY KEY,

    -- Soft and hard request-per-minute limits (NULL = use global default).
    rate_limit_per_minute  INTEGER,
    rate_limit_per_hour    INTEGER,

    -- Maximum number of active SSE connections for this tenant.
    max_sse_connections    INTEGER,

    -- Maximum events returned per paginated response.
    max_page_size          INTEGER,

    -- Bytes of event data this tenant may export per day (NULL = unlimited).
    max_export_bytes_day   BIGINT,

    -- Whether this tenant is currently suspended (all requests → 403).
    suspended              BOOLEAN     NOT NULL DEFAULT FALSE,
    suspended_reason       TEXT,
    suspended_at           TIMESTAMPTZ,

    created_at             TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Trigger to keep updated_at current on every UPDATE.
CREATE OR REPLACE FUNCTION touch_tenant_quotas_updated_at()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_tenant_quotas_updated_at ON tenant_quotas;
CREATE TRIGGER trg_tenant_quotas_updated_at
    BEFORE UPDATE ON tenant_quotas
    FOR EACH ROW EXECUTE FUNCTION touch_tenant_quotas_updated_at();

-- ─────────────────────────────────────────────────────────────────────────────
-- Row-Level Security hardening on tenant_access_audit
-- ─────────────────────────────────────────────────────────────────────────────
-- The audit table itself must be isolated: tenant A must not read tenant B's
-- audit rows, and no application role may delete or update rows.

ALTER TABLE tenant_access_audit ENABLE ROW LEVEL SECURITY;

-- Application role may INSERT and SELECT only their own tenant's rows.
-- Admin (superuser) bypasses RLS by default in PostgreSQL.
CREATE POLICY tenant_audit_isolation ON tenant_access_audit
    USING (
        tenant_id IS NULL
        OR current_setting('app.current_tenant_id', TRUE) = ''
        OR tenant_id = current_setting('app.current_tenant_id', TRUE)
    )
    WITH CHECK (
        tenant_id IS NULL
        OR current_setting('app.current_tenant_id', TRUE) = ''
        OR tenant_id = current_setting('app.current_tenant_id', TRUE)
    );

-- Revoke DELETE and UPDATE from the application role to make the table
-- effectively append-only.  Replace 'app_role' with your deployment role name.
-- REVOKE UPDATE, DELETE ON tenant_access_audit FROM app_role;
