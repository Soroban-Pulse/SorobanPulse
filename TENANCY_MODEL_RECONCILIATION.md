# Issue #1451 - Reconcile Dual Tenancy Models

## Problem Statement
Multi-tenancy is defined in two incompatible ways:
1. **SaaS Model**: `saas_tenants` table (VARCHAR(255) tenant_id) with quotas, API keys, billing
2. **RLS Model**: `tenant_id` UUID column on events table enforced via RLS policies and middleware/tenant.rs

The relationship, foreign keys, and lifecycle management between them are undefined.

## Current State

### SaaS Tenants Model
```sql
CREATE TABLE saas_tenants (
  id VARCHAR(255) PRIMARY KEY,          -- Tenant identifier (SaaS tenant slug)
  name VARCHAR(255),
  storage_quota_gb FLOAT,
  bandwidth_quota_gb FLOAT,
  api_calls_quota INTEGER,
  status VARCHAR,                        -- active, suspended, deleted
  created_at TIMESTAMP,
  updated_at TIMESTAMP
);
```

### Events RLS Model
```sql
-- events table partitioned by timestamp with tenant_id UUID
ALTER TABLE events ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON events
  USING (tenant_id = current_user_tenant_id);
```

## Issues

1. **Type Mismatch**: SaaS uses VARCHAR(255), RLS uses UUID
2. **No Foreign Key**: No link from events.tenant_id to saas_tenants.id
3. **Lifecycle**: Unclear what happens when saas_tenant is deleted
4. **Middleware**: tenant.rs enforces tenant isolation but doesn't reconcile models
5. **API Keys**: Stored with tenant_id but which model?

## Proposed Resolution

### Option A: Unify on UUID (Recommended)
```sql
-- Migrate saas_tenants
ALTER TABLE saas_tenants ADD COLUMN id_uuid UUID UNIQUE;
UPDATE saas_tenants SET id_uuid = gen_random_uuid();
-- After migration complete:
-- ALTER TABLE saas_tenants DROP COLUMN id;
-- ALTER TABLE saas_tenants RENAME COLUMN id_uuid TO id;
-- ALTER TABLE saas_tenants ALTER COLUMN id SET DEFAULT gen_random_uuid();

-- Add foreign key
ALTER TABLE events ADD CONSTRAINT fk_events_tenant
  FOREIGN KEY (tenant_id) REFERENCES saas_tenants(id) ON DELETE CASCADE;

-- Update middleware to use UUID consistently
-- Update API key storage to reference UUID tenant_id
```

**Benefits:**
- Single source of truth
- Proper referential integrity
- Cleaner cascade operations
- RLS works with UUID naturally

**Migration Path:**
1. Add id_uuid to saas_tenants
2. Add foreign key with CHECK constraint (defer until migration complete)
3. Migrate all references (api_keys, webhooks, audit_logs, subscriptions)
4. Drop old varchar id column
5. Rename id_uuid to id

### Option B: Keep Both, Explicit Mapping
```sql
-- Keep both columns
ALTER TABLE saas_tenants ADD COLUMN id_uuid UUID UNIQUE NOT NULL;

-- Create mapping table
CREATE TABLE tenant_id_mapping (
  saas_tenant_id VARCHAR(255) REFERENCES saas_tenants(id),
  rls_tenant_id UUID UNIQUE,
  created_at TIMESTAMP DEFAULT now()
);

-- Add foreign key
ALTER TABLE events ADD CONSTRAINT fk_events_tenant
  FOREIGN KEY (tenant_id) REFERENCES tenant_id_mapping(rls_tenant_id);
```

**Benefits:**
- Zero downtime migration
- Gradual transition possible
- Maintains backward compatibility

**Drawbacks:**
- Extra join required
- Two sources of truth
- Harder to maintain

## Recommended Decision

**Option A: Unify on UUID**

Rationale:
- RLS already uses UUID (better for distributed systems)
- Cleaner schema
- Proper referential integrity
- Easier to maintain long-term

## Implementation Steps

1. **Phase 1 - Preparation** (migration 20261120000001)
   - Add id_uuid to saas_tenants with DEFAULT gen_random_uuid()
   - Populate existing rows
   - Create unique constraint on id_uuid

2. **Phase 2 - Add FK** (migration 20261120000002)
   - Add NOT NULL constraint to events.tenant_id
   - Add FOREIGN KEY (tenant_id) REFERENCES saas_tenants(id_uuid)
   - Create unique index on saas_tenants(id_uuid)

3. **Phase 3 - Code Changes**
   - Update middleware/tenant.rs to use UUID
   - Update API key queries to reference UUID tenant_id
   - Update all SaaS tenant lookups to use id_uuid or drop old id column

4. **Phase 4 - Cleanup** (migration 20261120000003 or later)
   - Drop old saas_tenants.id column (after confirmed zero references)
   - Rename id_uuid to id

## Affected Code Files

- `middleware/tenant.rs` - Tenant extraction and validation
- `src/saas_platform.rs` - SaaS tenant CRUD
- `src/api_key_service.rs` - API key validation
- `src/subscription_service.rs` - Webhook subscriptions
- Migrations folder - Schema changes
- Tests - Update tenant creation fixtures

## Checklist

- [ ] Decide between Option A (UUID unification) or Option B (explicit mapping)
- [ ] Create Phase 1 migration (add id_uuid)
- [ ] Create Phase 2 migration (add FK constraint)
- [ ] Update middleware/tenant.rs to use UUID consistently
- [ ] Update all query builders to reference correct column
- [ ] Add tests for FK cascade (deletion)
- [ ] Update tenant fixtures in tests
- [ ] Document tenant model in architecture.md
- [ ] Create Phase 3 migration (drop old id column)
- [ ] Update API documentation for tenant field types

## Testing

```sql
-- Verify FK works
BEGIN;
DELETE FROM saas_tenants WHERE id = (SELECT DISTINCT tenant_id FROM events LIMIT 1);
-- Should fail with FK constraint violation
ROLLBACK;

-- Verify RLS + FK together
SET app.current_tenant_id = 'uuid-of-tenant-1';
SELECT COUNT(*) FROM events;  -- Should respect RLS + get tenant from saas_tenants
```
