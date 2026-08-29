-- Enhanced query caching with cache invalidation tracking (Issue #889)

CREATE TABLE IF NOT EXISTS query_cache_stats (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    cache_key TEXT NOT NULL UNIQUE,
    query_type TEXT NOT NULL,
    tenant_id TEXT DEFAULT 'default',
    hit_count BIGINT NOT NULL DEFAULT 0,
    miss_count BIGINT NOT NULL DEFAULT 0,
    last_hit TIMESTAMPTZ,
    last_miss TIMESTAMPTZ,
    entry_size_bytes BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_cache_stats_query_type ON query_cache_stats(query_type);
CREATE INDEX IF NOT EXISTS idx_cache_stats_tenant_id ON query_cache_stats(tenant_id);
CREATE INDEX IF NOT EXISTS idx_cache_stats_updated_at ON query_cache_stats(updated_at DESC);

-- Cache invalidation event log for tracking what triggered cache clears
CREATE TABLE IF NOT EXISTS cache_invalidation_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    trigger_event TEXT NOT NULL,
    affected_keys TEXT[] NOT NULL,
    tenant_id TEXT DEFAULT 'default',
    reason TEXT,
    triggered_by TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_invalidation_tenant_created ON cache_invalidation_log(tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_invalidation_trigger_event ON cache_invalidation_log(trigger_event);

-- Cache configuration table for admin control
CREATE TABLE IF NOT EXISTS cache_config (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL UNIQUE,
    ttl_seconds INT NOT NULL DEFAULT 300,
    max_size_mb INT NOT NULL DEFAULT 100,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    auto_warmup BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Insert default cache config
INSERT INTO cache_config (id, tenant_id) VALUES ('default', 'default')
ON CONFLICT (tenant_id) DO NOTHING;

CREATE INDEX IF NOT EXISTS idx_cache_config_tenant ON cache_config(tenant_id);
