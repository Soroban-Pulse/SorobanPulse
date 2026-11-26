# SorobanPulse Database Schema Reference

**Last Updated:** 2024-11-20  
**Total Migrations:** 125+  
**Version:** Latest  

---

## Overview

SorobanPulse uses PostgreSQL with the following key patterns:
- **Partitioning**: events table partitioned monthly by timestamp
- **Row-Level Security (RLS)**: All multi-tenant tables enforce tenant_id isolation
- **Indexing**: Strategic per-table and per-partition indexes
- **Materialized Views**: For performance-critical aggregations

---

## Core Tables

### events (Partitioned Parent)
```
Table: events
Type: PARTITIONED BY RANGE (timestamp)
Partition Scheme: Monthly (events_2024_01, events_2024_02, ...)
Archival: Rename to events_archive_YYYY_MM after 12 months
Retention: Keep archives 24 months before DELETE

Columns:
  event_id              BIGSERIAL PRIMARY KEY
  tenant_id             UUID NOT NULL (RLS POLICY)
  contract_id           VARCHAR(56) NOT NULL
  timestamp             TIMESTAMP WITH TIME ZONE NOT NULL
  event_type            VARCHAR(50) NOT NULL
  data                  JSONB
  created_at            TIMESTAMP DEFAULT now()

Indexes per partition:
  idx_events_YYYY_MM_tenant_id      (tenant_id)  [RLS filter]
  idx_events_YYYY_MM_contract_id    (contract_id, timestamp)
  idx_events_YYYY_MM_timestamp      (timestamp DESC)

RLS Policy:
  - SELECT: tenant_id = current_user_tenant_id
  - UPDATE: tenant_id = current_user_tenant_id
  - DELETE: tenant_id = current_user_tenant_id
  - Applies to: Parent + all partitions + archives
```

### subscriptions
```
Table: subscriptions
Type: REGULAR (non-partitioned)

Columns:
  id                    UUID PRIMARY KEY
  tenant_id             UUID NOT NULL (RLS POLICY)
  webhook_url           VARCHAR NOT NULL
  events_subscribed_to  TEXT[] (array of event types)
  status                VARCHAR (active, paused, deleted)
  created_at            TIMESTAMP DEFAULT now()
  updated_at            TIMESTAMP DEFAULT now()

Indexes:
  idx_subscriptions_tenant_id       (tenant_id)
  idx_subscriptions_status          (status) WHERE status = 'active'

RLS Policy:
  - SELECT/UPDATE/DELETE: tenant_id = current_user_tenant_id
```

### audit_logs
```
Table: audit_logs
Type: REGULAR (non-partitioned)

Columns:
  id                    BIGSERIAL PRIMARY KEY
  tenant_id             UUID NOT NULL (RLS POLICY)
  user_id               UUID
  action                VARCHAR (CREATE, UPDATE, DELETE, READ)
  table_name            VARCHAR
  record_id             UUID
  old_values            JSONB
  new_values            JSONB
  timestamp             TIMESTAMP DEFAULT now()

Indexes:
  idx_audit_logs_tenant_id          (tenant_id)
  idx_audit_logs_timestamp          (timestamp DESC)
  idx_audit_logs_user_id            (user_id, timestamp DESC)

RLS Policy:
  - SELECT: tenant_id = current_user_tenant_id
```

---

## Multi-Tenant Configuration Tables

### tenants (SaaS)
```
Table: tenants
Type: REGULAR (not RLS - shared admin view)

Columns:
  id                    UUID PRIMARY KEY
  name                  VARCHAR NOT NULL
  slug                  VARCHAR UNIQUE
  storage_quota_gb      FLOAT DEFAULT 1.0
  bandwidth_quota_gb    FLOAT DEFAULT 10.0
  api_calls_quota       INTEGER DEFAULT 1000000
  status                VARCHAR (active, suspended, deleted)
  created_at            TIMESTAMP DEFAULT now()

Indexes:
  idx_tenants_slug      (slug)
  idx_tenants_status    (status)
```

### api_keys
```
Table: api_keys
Type: REGULAR (with RLS)

Columns:
  id                    UUID PRIMARY KEY
  tenant_id             UUID NOT NULL (RLS POLICY)
  key_hash              VARCHAR NOT NULL UNIQUE (SHA256)
  key_prefix            VARCHAR(10) (for logs, e.g., "sk_1234")
  name                  VARCHAR
  expires_at            TIMESTAMP
  last_used_at          TIMESTAMP
  created_at            TIMESTAMP DEFAULT now()

Indexes:
  idx_api_keys_tenant_id            (tenant_id)
  idx_api_keys_key_hash             (key_hash)

RLS Policy:
  - SELECT/UPDATE: tenant_id = current_user_tenant_id
```

---

## Materialized Views (Performance)

### events_by_contract_daily
```
Definition:
  SELECT
    contract_id,
    DATE(timestamp) as day,
    COUNT(*) as event_count,
    COUNT(DISTINCT tenant_id) as tenant_count
  FROM events
  GROUP BY contract_id, DATE(timestamp)

Refresh Strategy: CONCURRENTLY (non-blocking)
Refresh Frequency: Hourly
Used by: Dashboard, analytics endpoints
```

### tenant_usage_summary
```
Definition:
  SELECT
    tenant_id,
    COUNT(*) as total_events,
    COUNT(DISTINCT contract_id) as contracts,
    MIN(timestamp) as first_event,
    MAX(timestamp) as last_event,
    SUM(CAST(data->>'bytes_transferred' AS INTEGER)) / 1000000.0 as bandwidth_gb
  FROM events
  GROUP BY tenant_id

Refresh Strategy: CONCURRENTLY
Refresh Frequency: Daily (02:00 UTC)
Used by: SaaS billing, usage dashboard
```

---

## Partitioning Strategy Detail

### Creation
```sql
-- Parent table (created once)
CREATE TABLE events (
  event_id BIGSERIAL,
  tenant_id UUID NOT NULL,
  contract_id VARCHAR(56),
  timestamp TIMESTAMP WITH TIME ZONE,
  event_type VARCHAR(50),
  data JSONB,
  PRIMARY KEY (event_id, timestamp)
) PARTITION BY RANGE (timestamp);

-- Partition created monthly by partition_manager
CREATE TABLE events_2024_01 PARTITION OF events
  FOR VALUES FROM ('2024-01-01') TO ('2024-02-01');
```

### Archival
```sql
-- After 12 months (archival_age_months=12)
ALTER TABLE events_2024_01 RENAME TO events_archive_2024_01;

-- RLS policies apply to archived table automatically
-- (created as part of table definition, not column-level)
```

### Deletion
```sql
-- After 24 months total (retention_months=24)
DROP TABLE events_archive_2024_01;
```

---

## RLS Policies Summary

| Table | Policy | Scope |
|-------|--------|-------|
| events | tenant_id = current_user_tenant_id | Parent + all partitions + archives |
| subscriptions | tenant_id = current_user_tenant_id | Table |
| audit_logs | tenant_id = current_user_tenant_id | Table |
| api_keys | tenant_id = current_user_tenant_id | Table |

**Admin Override**: SET ROLE to admin role to bypass RLS

---

## Migration History (Selected)

| Migration | Date | Purpose |
|-----------|------|---------|
| 20260527000001_rls_events.sql | 2026-05-27 | Initial RLS policies |
| 20260727000102_partition_events_by_month.sql | 2026-07-27 | Add monthly partitioning |
| 20261022000001_rls_events_partitioned.sql | 2026-10-22 | Re-apply RLS to partitions |
| 20261120000001_partition_archival.sql | 2026-11-20 | Archival automation |

---

## Related Documentation

- **Architecture**: docs/architecture.md
- **Data Retention**: docs/data-retention.md
- **Partition Management**: docs/partition-management.md
- **Performance Tuning**: docs/performance-tuning.md

---

## ER Diagram

[Auto-generated via pgmodeler - see docs/schema-diagram/]

## Regenerating This Document

Run: `scripts/generate-schema-docs.sh`

This regenerates the schema reference from live database metadata.
