-- Issue #814: operator feedback on anomaly alerts, used to auto-tune thresholds.
CREATE TABLE IF NOT EXISTS anomaly_alert_feedback (
    id UUID PRIMARY KEY,
    alert_id UUID NOT NULL REFERENCES anomaly_alerts(id) ON DELETE CASCADE,
    feedback TEXT NOT NULL CHECK (feedback IN ('true_positive', 'false_positive', 'missed')),
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_anomaly_alert_feedback_alert
    ON anomaly_alert_feedback (alert_id, created_at DESC);
