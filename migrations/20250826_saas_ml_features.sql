-- Migration for SaaS Platform & Multi-Tenant Hosting (Issue #841)
-- and AI/ML Integration & Intelligence Features (Issue #842)

-- Create enum types for SaaS platform
CREATE TYPE subscription_tier AS ENUM ('free', 'starter', 'professional', 'enterprise', 'custom');
CREATE TYPE tenant_status AS ENUM ('active', 'suspended', 'pending', 'cancelled', 'trial');

-- SaaS tenants table
CREATE TABLE IF NOT EXISTS saas_tenants (
    id UUID PRIMARY KEY,
    tenant_id VARCHAR(255) UNIQUE NOT NULL,
    organization_name VARCHAR(255) NOT NULL,
    contact_email VARCHAR(255) NOT NULL,
    subscription_tier subscription_tier NOT NULL DEFAULT 'free',
    status tenant_status NOT NULL DEFAULT 'pending',
    trial_ends_at TIMESTAMP WITH TIME ZONE,
    subscription_started_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    subscription_renewed_at TIMESTAMP WITH TIME ZONE,
    custom_domain VARCHAR(255),
    max_api_requests_per_day BIGINT NOT NULL DEFAULT 1000,
    max_events_per_month BIGINT NOT NULL DEFAULT 10000,
    max_subscriptions INTEGER NOT NULL DEFAULT 5,
    max_webhooks INTEGER NOT NULL DEFAULT 2,
    storage_quota_gb INTEGER NOT NULL DEFAULT 1,
    sla_uptime_percentage DOUBLE PRECISION NOT NULL DEFAULT 95.0,
    dedicated_support BOOLEAN NOT NULL DEFAULT FALSE,
    custom_branding BOOLEAN NOT NULL DEFAULT FALSE,
    api_keys TEXT[] NOT NULL DEFAULT '{}',
    billing_email VARCHAR(255),
    billing_id VARCHAR(255),
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_saas_tenants_tenant_id ON saas_tenants(tenant_id);
CREATE INDEX idx_saas_tenants_status ON saas_tenants(status);
CREATE INDEX idx_saas_tenants_tier ON saas_tenants(subscription_tier);
CREATE INDEX idx_saas_tenants_trial_ends ON saas_tenants(trial_ends_at) WHERE trial_ends_at IS NOT NULL;

-- API request logs for usage tracking
CREATE TABLE IF NOT EXISTS api_request_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id VARCHAR(255) NOT NULL,
    endpoint VARCHAR(500) NOT NULL,
    method VARCHAR(10) NOT NULL,
    status_code INTEGER,
    response_time_ms INTEGER,
    timestamp TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    user_agent TEXT,
    ip_address INET
);

CREATE INDEX idx_api_request_logs_tenant ON api_request_logs(tenant_id, timestamp DESC);
CREATE INDEX idx_api_request_logs_timestamp ON api_request_logs(timestamp DESC);

-- Webhook deliveries for tracking
CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id VARCHAR(255),
    subscription_id UUID,
    webhook_url TEXT NOT NULL,
    payload JSONB NOT NULL,
    status_code INTEGER,
    success BOOLEAN NOT NULL DEFAULT FALSE,
    attempts INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    delivered_at TIMESTAMP WITH TIME ZONE
);

CREATE INDEX idx_webhook_deliveries_tenant ON webhook_deliveries(tenant_id, created_at DESC);

-- ML models table
CREATE TABLE IF NOT EXISTS ml_models (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    model_type VARCHAR(100) NOT NULL,
    version VARCHAR(50) NOT NULL,
    accuracy DOUBLE PRECISION,
    precision DOUBLE PRECISION,
    recall DOUBLE PRECISION,
    f1_score DOUBLE PRECISION,
    training_samples BIGINT NOT NULL,
    last_trained_at TIMESTAMP WITH TIME ZONE NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    hyperparameters JSONB NOT NULL DEFAULT '{}',
    feature_importance JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ml_models_type ON ml_models(model_type);
CREATE INDEX idx_ml_models_active ON ml_models(is_active);
CREATE INDEX idx_ml_models_name ON ml_models(name);

-- Detected patterns table
CREATE TABLE IF NOT EXISTS detected_patterns (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pattern_type VARCHAR(100) NOT NULL,
    confidence DOUBLE PRECISION NOT NULL,
    description TEXT NOT NULL,
    affected_contracts TEXT[] NOT NULL DEFAULT '{}',
    frequency BIGINT NOT NULL,
    first_seen TIMESTAMP WITH TIME ZONE NOT NULL,
    last_seen TIMESTAMP WITH TIME ZONE NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_detected_patterns_type ON detected_patterns(pattern_type);
CREATE INDEX idx_detected_patterns_confidence ON detected_patterns(confidence DESC);
CREATE INDEX idx_detected_patterns_last_seen ON detected_patterns(last_seen DESC);

-- ML predictions table
CREATE TABLE IF NOT EXISTS ml_predictions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    model_id UUID NOT NULL REFERENCES ml_models(id),
    prediction_type VARCHAR(100) NOT NULL,
    predicted_value DOUBLE PRECISION NOT NULL,
    confidence_lower DOUBLE PRECISION NOT NULL,
    confidence_upper DOUBLE PRECISION NOT NULL,
    confidence_score DOUBLE PRECISION NOT NULL,
    feature_values JSONB NOT NULL,
    prediction_time TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    valid_until TIMESTAMP WITH TIME ZONE NOT NULL,
    actual_value DOUBLE PRECISION,
    error DOUBLE PRECISION
);

CREATE INDEX idx_ml_predictions_model ON ml_predictions(model_id, prediction_time DESC);
CREATE INDEX idx_ml_predictions_type ON ml_predictions(prediction_type);
CREATE INDEX idx_ml_predictions_valid ON ml_predictions(valid_until) WHERE valid_until > NOW();

-- Intelligent filters table
CREATE TABLE IF NOT EXISTS intelligent_filters (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    description TEXT,
    conditions JSONB NOT NULL,
    action JSONB NOT NULL,
    confidence DOUBLE PRECISION NOT NULL,
    auto_learned BOOLEAN NOT NULL DEFAULT FALSE,
    true_positives BIGINT NOT NULL DEFAULT 0,
    false_positives BIGINT NOT NULL DEFAULT 0,
    true_negatives BIGINT NOT NULL DEFAULT 0,
    false_negatives BIGINT NOT NULL DEFAULT 0,
    precision DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    recall DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    f1_score DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_intelligent_filters_active ON intelligent_filters(is_active);
CREATE INDEX idx_intelligent_filters_confidence ON intelligent_filters(confidence DESC);

-- Add tenant_id to events table if not exists
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'events' AND column_name = 'tenant_id'
    ) THEN
        ALTER TABLE events ADD COLUMN tenant_id VARCHAR(255);
        CREATE INDEX idx_events_tenant_id ON events(tenant_id) WHERE tenant_id IS NOT NULL;
    END IF;
END $$;

-- Add tenant_id to subscriptions table if not exists
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'subscriptions' AND column_name = 'tenant_id'
    ) THEN
        ALTER TABLE subscriptions ADD COLUMN tenant_id VARCHAR(255);
        CREATE INDEX idx_subscriptions_tenant_id ON subscriptions(tenant_id) WHERE tenant_id IS NOT NULL;
    END IF;
END $$;

-- Metric history for ML training (if not exists)
CREATE TABLE IF NOT EXISTS metric_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    subscription_id UUID,
    metric_name VARCHAR(255) NOT NULL,
    metric_value DOUBLE PRECISION,
    timestamp TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    tenant_id VARCHAR(255)
);

CREATE INDEX idx_metric_history_subscription ON metric_history(subscription_id, metric_name, timestamp DESC);
CREATE INDEX idx_metric_history_tenant ON metric_history(tenant_id, timestamp DESC) WHERE tenant_id IS NOT NULL;

-- Function to update updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Triggers for updated_at
CREATE TRIGGER update_saas_tenants_updated_at 
    BEFORE UPDATE ON saas_tenants 
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_ml_models_updated_at 
    BEFORE UPDATE ON ml_models 
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_intelligent_filters_updated_at 
    BEFORE UPDATE ON intelligent_filters 
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Comments for documentation
COMMENT ON TABLE saas_tenants IS 'SaaS platform tenants with subscription and billing information (Issue #841)';
COMMENT ON TABLE ml_models IS 'Machine learning models for anomaly detection and pattern recognition (Issue #842)';
COMMENT ON TABLE detected_patterns IS 'Patterns detected by ML algorithms in event data (Issue #842)';
COMMENT ON TABLE ml_predictions IS 'Predictions made by ML models with confidence intervals (Issue #842)';
COMMENT ON TABLE intelligent_filters IS 'Auto-learned intelligent filters with performance metrics (Issue #842)';
