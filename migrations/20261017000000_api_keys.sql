-- Issue #1258: scoped API keys stored hashed in the database.
--
-- Keys are generated as `sp_<base58(32 random bytes)>` and only their
-- digest is stored. Revocation is a soft delete (`revoked_at`)
-- so audit history is kept and a revoked key can never be re-activated.
--
-- Issue #1430: `key_hash` is now HMAC-SHA256(API_KEY_PEPPER, key) rendered as
-- lowercase hex, rather than a bare SHA-256 digest. The pepper is a
-- server-side secret loaded from the environment; without it the stored
-- digests cannot be brute-forced offline. The digest is still 64 hex
-- characters, so the format check below is unchanged. Existing bare
-- SHA-256 digests are re-hashed transparently on next use (see
-- `src/middleware/auth.rs`).
--
-- Issue #1429: tenant-bound keys (formerly stored in plaintext in
-- `saas_tenants.api_keys`) live here with `tenant_id` set, so SaaS tenant
-- authentication goes through the same hashed-key lookup as every other
-- key. No table stores raw API keys.

CREATE TABLE IF NOT EXISTS api_keys (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name         TEXT NOT NULL,
    -- HMAC-SHA256(API_KEY_PEPPER, key) as lowercase hex (64 chars).
    key_hash     TEXT NOT NULL UNIQUE,
    scopes       TEXT[] NOT NULL DEFAULT '{}',
    -- Optional tenant binding for multi-tenant deployments.
    tenant_id    TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ,
    revoked_at   TIMESTAMPTZ,

    CONSTRAINT api_keys_name_not_blank CHECK (length(btrim(name)) > 0),
    CONSTRAINT api_keys_key_hash_sha256 CHECK (key_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT api_keys_scopes_known CHECK (
        scopes <@ ARRAY['read:events', 'write:subscriptions', 'admin']::TEXT[]
    )
);

CREATE INDEX IF NOT EXISTS idx_api_keys_created_at ON api_keys (created_at DESC);

-- Issue #1429: fast lookup of all keys bound to a tenant.
CREATE INDEX IF NOT EXISTS idx_api_keys_tenant_id ON api_keys (tenant_id);