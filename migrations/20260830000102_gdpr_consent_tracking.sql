-- Migration: GDPR Consent and Data Subject Request Tracking (Issue #945)

-- Table for tracking data subject consent
CREATE TABLE IF NOT EXISTS gdpr_consent_records (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  subject_email TEXT NOT NULL,
  consent_type TEXT NOT NULL,      -- 'marketing', 'analytics', 'notifications'
  granted BOOLEAN NOT NULL DEFAULT FALSE,
  granted_at TIMESTAMPTZ,
  withdrawn_at TIMESTAMPTZ,
  ip_address INET,
  user_agent TEXT,
  source TEXT,                     -- 'signup', 'preference_centre', 'api'
  legal_basis TEXT,                -- 'consent', 'legitimate_interest', 'contract'
  notes TEXT,
  created_at TIMESTAMPTZ DEFAULT NOW(),
  updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_gdpr_consent_email ON gdpr_consent_records (subject_email);
CREATE INDEX IF NOT EXISTS idx_gdpr_consent_type ON gdpr_consent_records (consent_type);
CREATE INDEX IF NOT EXISTS idx_gdpr_consent_granted ON gdpr_consent_records (granted);

-- Table for tracking GDPR data subject requests (DSR)
CREATE TABLE IF NOT EXISTS gdpr_data_subject_requests (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  request_type TEXT NOT NULL,       -- 'access', 'erasure', 'rectification', 'portability', 'restriction'
  subject_email TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending',  -- 'pending', 'in_progress', 'completed', 'rejected'
  requested_at TIMESTAMPTZ DEFAULT NOW(),
  deadline_at TIMESTAMPTZ GENERATED ALWAYS AS (requested_at + INTERVAL '30 days') STORED,
  completed_at TIMESTAMPTZ,
  handled_by TEXT,
  rejection_reason TEXT,
  notes TEXT,
  proof_of_completion JSONB          -- references to actions taken
);

CREATE INDEX IF NOT EXISTS idx_gdpr_dsr_email ON gdpr_data_subject_requests (subject_email);
CREATE INDEX IF NOT EXISTS idx_gdpr_dsr_status ON gdpr_data_subject_requests (status);
CREATE INDEX IF NOT EXISTS idx_gdpr_dsr_deadline ON gdpr_data_subject_requests (deadline_at)
  WHERE status != 'completed' AND status != 'rejected';

-- Table for tracking breach notifications (Article 33/34)
CREATE TABLE IF NOT EXISTS gdpr_breach_notifications (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  detected_at TIMESTAMPTZ NOT NULL,
  notified_authority_at TIMESTAMPTZ,  -- must be within 72 hours of detection
  notified_subjects_at TIMESTAMPTZ,
  breach_type TEXT NOT NULL,          -- 'confidentiality', 'integrity', 'availability'
  data_categories TEXT[],             -- types of personal data affected
  affected_subject_count INTEGER,
  description TEXT NOT NULL,
  containment_measures TEXT,
  likely_consequences TEXT,
  dpa_reference TEXT,                 -- reference number from supervisory authority
  resolved_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Function to get all personal data for a subject (Article 15)
CREATE OR REPLACE FUNCTION gdpr_get_subject_data(p_email TEXT)
RETURNS JSONB
LANGUAGE SQL
STABLE
AS $$
  SELECT jsonb_build_object(
    'email', p_email,
    'generated_at', NOW(),
    'subscriptions', (
      SELECT COALESCE(jsonb_agg(row_to_json(s)), '[]'::jsonb)
      FROM subscriptions s WHERE s.email = p_email
    ),
    'consent_records', (
      SELECT COALESCE(jsonb_agg(row_to_json(c)), '[]'::jsonb)
      FROM gdpr_consent_records c WHERE c.subject_email = p_email
    ),
    'data_subject_requests', (
      SELECT COALESCE(jsonb_agg(row_to_json(d)), '[]'::jsonb)
      FROM gdpr_data_subject_requests d WHERE d.subject_email = p_email
    )
  );
$$;
