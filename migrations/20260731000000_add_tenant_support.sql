-- Add tenant_id column to events table for multi-tenant isolation (Issue #887)
ALTER TABLE events ADD COLUMN IF NOT EXISTS tenant_id TEXT NOT NULL DEFAULT 'default';

-- Create index on tenant_id for efficient filtering
CREATE INDEX IF NOT EXISTS idx_events_tenant_id ON events(tenant_id);

-- Create composite index for tenant + contract_id queries
CREATE INDEX IF NOT EXISTS idx_events_tenant_contract ON events(tenant_id, contract_id);

-- Create composite index for tenant + ledger queries
CREATE INDEX IF NOT EXISTS idx_events_tenant_ledger ON events(tenant_id, ledger DESC);

-- Create composite index for tenant + timestamp queries
CREATE INDEX IF NOT EXISTS idx_events_tenant_timestamp ON events(tenant_id, timestamp DESC);

-- Create Row Level Security (RLS) policy table if not exists
CREATE TABLE IF NOT EXISTS tenant_rls_policies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id TEXT NOT NULL UNIQUE,
    policy_json JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_tenant_rls_tenant_id ON tenant_rls_policies(tenant_id);

-- Create tenant metadata table
CREATE TABLE IF NOT EXISTS tenants (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    admin_email TEXT,
    config JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    enabled BOOLEAN NOT NULL DEFAULT TRUE
);

-- Insert default tenant
INSERT INTO tenants (id, name, enabled) VALUES ('default', 'Default Tenant', TRUE)
ON CONFLICT (id) DO NOTHING;

CREATE INDEX IF NOT EXISTS idx_tenants_created_at ON tenants(created_at);
