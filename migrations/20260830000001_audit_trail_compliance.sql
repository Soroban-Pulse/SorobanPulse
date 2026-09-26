-- Migration: Audit Trail Compliance Enhancements (Issue #946)
-- Extends audit_logs for immutability, signing, retention, and compliance reporting

-- Add compliance-specific columns to existing audit_logs table
ALTER TABLE audit_logs
  ADD COLUMN IF NOT EXISTS log_hash TEXT,
  ADD COLUMN IF NOT EXISTS chain_hash TEXT,
  ADD COLUMN IF NOT EXISTS signed_at TIMESTAMPTZ,
  ADD COLUMN IF NOT EXISTS compliance_tags TEXT[] DEFAULT '{}',
  ADD COLUMN IF NOT EXISTS retention_class TEXT DEFAULT 'standard',
  ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ,
  ADD COLUMN IF NOT EXISTS export_ref TEXT;

-- Index for compliance queries
CREATE INDEX IF NOT EXISTS idx_audit_logs_compliance_tags
  ON audit_logs USING GIN (compliance_tags);

CREATE INDEX IF NOT EXISTS idx_audit_logs_retention_class
  ON audit_logs (retention_class);

CREATE INDEX IF NOT EXISTS idx_audit_logs_signed_at
  ON audit_logs (signed_at)
  WHERE signed_at IS NOT NULL;

-- Table for compliance report runs
CREATE TABLE IF NOT EXISTS compliance_report_runs (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  report_type TEXT NOT NULL,         -- 'audit_trail', 'gdpr', 'soc2'
  period_from TIMESTAMPTZ NOT NULL,
  period_to TIMESTAMPTZ NOT NULL,
  generated_by TEXT,
  summary JSONB,
  created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_compliance_report_runs_type
  ON compliance_report_runs (report_type, created_at DESC);

-- Table for audit log exports (tracks what was exported and when)
CREATE TABLE IF NOT EXISTS audit_log_exports (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  exported_by TEXT NOT NULL,
  export_format TEXT NOT NULL DEFAULT 'json',
  filter_params JSONB,
  row_count INTEGER,
  file_hash TEXT,
  created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Function to compute a simple chain hash for audit log tamper detection
CREATE OR REPLACE FUNCTION compute_audit_chain_hash(p_id UUID)
RETURNS TEXT
LANGUAGE SQL
STABLE
AS $$
  SELECT encode(
    sha256(
      (COALESCE(al.chain_hash, '') || '|' ||
       al.id::TEXT || '|' ||
       al.event_type || '|' ||
       al.action || '|' ||
       al.created_at::TEXT
      )::BYTEA
    ),
    'hex'
  )
  FROM audit_logs al
  WHERE al.id = p_id;
$$;

-- Retention policy view for compliance officers
CREATE OR REPLACE VIEW v_audit_retention_summary AS
SELECT
  retention_class,
  severity,
  COUNT(*) AS total_records,
  MIN(created_at) AS oldest_record,
  MAX(created_at) AS newest_record,
  COUNT(*) FILTER (WHERE expires_at < NOW()) AS expired_count,
  COUNT(*) FILTER (WHERE archived_at IS NOT NULL) AS archived_count
FROM audit_logs
GROUP BY retention_class, severity
ORDER BY retention_class, severity;
