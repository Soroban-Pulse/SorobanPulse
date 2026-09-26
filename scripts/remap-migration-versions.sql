-- One-off upgrade: remap renamed migration versions in _sqlx_migrations (issue #1051).
-- Run once against deployed databases BEFORE starting the new binary.
BEGIN;
UPDATE _sqlx_migrations SET version = 20260527000100 WHERE version = 20260527000000 AND description = 'indexer bloom state';
UPDATE _sqlx_migrations SET version = 20260530000101 WHERE version = 20260530000001 AND description = 'add notification channels';
UPDATE _sqlx_migrations SET version = 20260530000102 WHERE version = 20260530000002 AND description = 'add saved queries';
UPDATE _sqlx_migrations SET version = 20260530000201 WHERE version = 20260530000001 AND description = 'mv contract summary';
UPDATE _sqlx_migrations SET version = 20260625000100 WHERE version = 20260625000000 AND description = 'notification failover and costs';
UPDATE _sqlx_migrations SET version = 20260625000101 WHERE version = 20260625000001 AND description = 'add notification channel versions';
UPDATE _sqlx_migrations SET version = 20260625000102 WHERE version = 20260625000002 AND description = 'notification audit log';
UPDATE _sqlx_migrations SET version = 20260625000201 WHERE version = 20260625000001 AND description = 'maintenance windows';
UPDATE _sqlx_migrations SET version = 20260625000202 WHERE version = 20260625000002 AND description = 'notification channel groups';
UPDATE _sqlx_migrations SET version = 20260625000301 WHERE version = 20260625000001 AND description = 'notification channel enhancements';
UPDATE _sqlx_migrations SET version = 20260625000401 WHERE version = 20260625000001 AND description = 'notification deliveries';
UPDATE _sqlx_migrations SET version = 20260627000101 WHERE version = 20260627000001 AND description = 'feature flags';
UPDATE _sqlx_migrations SET version = 20260629000101 WHERE version = 20260629000001 AND description = 'sse reconnect and query cache';
UPDATE _sqlx_migrations SET version = 20260629000201 WHERE version = 20260629000001 AND description = 'webhook retry queue';
UPDATE _sqlx_migrations SET version = 20260630000102 WHERE version = 20260630000002 AND description = 'subscription email push';
UPDATE _sqlx_migrations SET version = 20260727000101 WHERE version = 20260727000001 AND description = 'webhook templates';
UPDATE _sqlx_migrations SET version = 20260727000102 WHERE version = 20260727000002 AND description = 'partition events by month';
UPDATE _sqlx_migrations SET version = 20260727000103 WHERE version = 20260727000003 AND description = 'statistics auto analysis';
UPDATE _sqlx_migrations SET version = 20260729000101 WHERE version = 20260729000001 AND description = 'partition management functions';
UPDATE _sqlx_migrations SET version = 20260729000201 WHERE version = 20260729000001 AND description = 'tenant access audit';
UPDATE _sqlx_migrations SET version = 20260830000101 WHERE version = 20260830000001 AND description = 'event grouping aggregation';
UPDATE _sqlx_migrations SET version = 20260830000102 WHERE version = 20260830000002 AND description = 'gdpr consent tracking';
UPDATE _sqlx_migrations SET version = 20260831000101 WHERE version = 20260831000001 AND description = 'webhook request response logging';
COMMIT;
