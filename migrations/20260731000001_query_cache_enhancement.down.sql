-- Rollback query cache enhancement
DROP INDEX IF EXISTS idx_cache_config_tenant;
DROP TABLE IF EXISTS cache_config;

DROP INDEX IF EXISTS idx_invalidation_trigger_event;
DROP INDEX IF EXISTS idx_invalidation_tenant_created;
DROP TABLE IF EXISTS cache_invalidation_log;

DROP INDEX IF EXISTS idx_cache_stats_updated_at;
DROP INDEX IF EXISTS idx_cache_stats_tenant_id;
DROP INDEX IF EXISTS idx_cache_stats_query_type;
DROP TABLE IF EXISTS query_cache_stats;
