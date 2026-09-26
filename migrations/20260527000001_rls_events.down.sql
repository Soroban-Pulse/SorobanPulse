-- Down migration for 20260527000001_rls_events.sql
DROP POLICY IF EXISTS tenant_isolation ON events;
ALTER TABLE events DISABLE ROW LEVEL SECURITY;
