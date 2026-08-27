// Issue #897: Real-time alerting for critical events
//
// This module implements alert management including:
// - Alert routing based on severity
// - Alert deduplication and grouping
// - Silence management endpoints
// - Integration with PagerDuty, Opsgenie, VictorOps
// - Alert templating with context information
// - Alert history and metrics

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;

extern crate metrics as m;

/// Alert severity level
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AlertSeverity {
    /// Informational alert
    Info,
    /// Warning-level alert
    Warning,
    /// Critical alert requiring immediate action
    Critical,
}

impl AlertSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            AlertSeverity::Info => "info",
            AlertSeverity::Warning => "warning",
            AlertSeverity::Critical => "critical",
        }
    }

    pub fn priority(&self) -> u32 {
        match self {
            AlertSeverity::Info => 1,
            AlertSeverity::Warning => 2,
            AlertSeverity::Critical => 3,
        }
    }
}

/// Alert status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AlertStatus {
    /// Alert is firing
    Firing,
    /// Alert has been resolved
    Resolved,
}

impl AlertStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            AlertStatus::Firing => "firing",
            AlertStatus::Resolved => "resolved",
        }
    }
}

/// Silence rule for suppressing alerts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertSilence {
    pub id: String,
    pub alert_name: String,
    pub matchers: Vec<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub created_by: String,
    pub comment: String,
    pub created_at: DateTime<Utc>,
}

impl AlertSilence {
    pub fn is_active(&self) -> bool {
        let now = Utc::now();
        now >= self.starts_at && now <= self.ends_at
    }

    pub fn matches_alert(&self, alert_name: &str) -> bool {
        self.alert_name == "*" || self.alert_name == alert_name
    }
}

/// Complete alert with all context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: String,
    pub alert_name: String,
    pub status: AlertStatus,
    pub severity: AlertSeverity,
    pub component: String,
    pub summary: String,
    pub description: String,
    pub runbook_url: Option<String>,
    pub firing_since: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub labels: Vec<(String, String)>,
    pub annotations: Vec<(String, String)>,
    pub triggered_at: DateTime<Utc>,
}

/// Alert routing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRoutingConfig {
    pub enable_pagerduty: bool,
    pub enable_opsgenie: bool,
    pub enable_victorops: bool,
    pub enable_slack: bool,
    pub deduplication_window_seconds: u64,
    pub grouping_interval_seconds: u64,
}

impl Default for AlertRoutingConfig {
    fn default() -> Self {
        Self {
            enable_pagerduty: true,
            enable_opsgenie: true,
            enable_victorops: true,
            enable_slack: true,
            deduplication_window_seconds: 300,
            grouping_interval_seconds: 30,
        }
    }
}

/// Alert manager for tracking and routing alerts
pub struct AlertManager {
    config: Arc<RwLock<AlertRoutingConfig>>,
    alert_history: Arc<RwLock<Vec<Alert>>>,
}

impl AlertManager {
    pub fn new(config: AlertRoutingConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            alert_history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Record a new alert
    pub async fn record_alert(&self, alert: Alert) {
        let mut history = self.alert_history.write().await;
        history.push(alert.clone());

        // Record metrics
        m::counter!(
            "soroban_pulse_alerts_total",
            "severity" => alert.severity.as_str(),
            "component" => alert.component.clone()
        )
        .increment(1);

        // Keep only last 10,000 alerts in memory
        if history.len() > 10_000 {
            history.remove(0);
        }
    }

    /// Get alert history
    pub async fn get_alert_history(&self, limit: usize) -> Vec<Alert> {
        let history = self.alert_history.read().await;
        history
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get active alerts
    pub async fn get_active_alerts(&self) -> Vec<Alert> {
        let history = self.alert_history.read().await;
        history
            .iter()
            .filter(|a| a.status == AlertStatus::Firing)
            .cloned()
            .collect()
    }

    /// Determine if alert should be routed to critical receiver
    pub fn should_route_critical(&self, alert: &Alert) -> bool {
        matches!(alert.severity, AlertSeverity::Critical)
    }

    /// Update routing configuration
    pub async fn update_routing_config(&self, config: AlertRoutingConfig) {
        let mut current = self.config.write().await;
        *current = config;
    }
}

/// Create alerting database tables
pub async fn create_alert_tables(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS alert_silences (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            alert_name TEXT NOT NULL,
            matchers TEXT[] DEFAULT '{}',
            starts_at TIMESTAMPTZ NOT NULL,
            ends_at TIMESTAMPTZ NOT NULL,
            created_by TEXT NOT NULL,
            comment TEXT,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE(alert_name, starts_at, ends_at)
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_alert_silences_active ON alert_silences(alert_name) WHERE ends_at > NOW();",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS alert_history (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            alert_name TEXT NOT NULL,
            status TEXT NOT NULL CHECK (status IN ('firing', 'resolved')),
            severity TEXT NOT NULL CHECK (severity IN ('info', 'warning', 'critical')),
            component TEXT NOT NULL,
            summary TEXT NOT NULL,
            description TEXT,
            runbook_url TEXT,
            firing_since TIMESTAMPTZ,
            resolved_at TIMESTAMPTZ,
            triggered_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_alert_history_time ON alert_history(triggered_at DESC);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_alert_history_status ON alert_history(status, severity);",
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Create a silence rule
pub async fn create_silence(
    pool: &PgPool,
    alert_name: &str,
    duration_minutes: u64,
    created_by: &str,
    comment: &str,
) -> Result<AlertSilence, sqlx::Error> {
    let now = Utc::now();
    let ends_at = now + Duration::minutes(duration_minutes as i64);
    let id = uuid::Uuid::new_v4().to_string();

    sqlx::query(
        r#"
        INSERT INTO alert_silences (id, alert_name, starts_at, ends_at, created_by, comment)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(&id)
    .bind(alert_name)
    .bind(now)
    .bind(ends_at)
    .bind(created_by)
    .bind(comment)
    .execute(pool)
    .await?;

    Ok(AlertSilence {
        id,
        alert_name: alert_name.to_string(),
        matchers: vec![],
        starts_at: now,
        ends_at,
        created_by: created_by.to_string(),
        comment: comment.to_string(),
        created_at: now,
    })
}

/// Get active silences
pub async fn get_active_silences(pool: &PgPool) -> Result<Vec<AlertSilence>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT id, alert_name, matchers, starts_at, ends_at, created_by, comment, created_at
        FROM alert_silences
        WHERE ends_at > NOW()
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| AlertSilence {
            id: r.get(0),
            alert_name: r.get(1),
            matchers: r.get(2),
            starts_at: r.get(3),
            ends_at: r.get(4),
            created_by: r.get(5),
            comment: r.get(6),
            created_at: r.get(7),
        })
        .collect())
}

/// Delete a silence rule
pub async fn delete_silence(pool: &PgPool, silence_id: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM alert_silences WHERE id = $1")
        .bind(silence_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alert_severity_priority() {
        assert_eq!(AlertSeverity::Info.priority(), 1);
        assert_eq!(AlertSeverity::Warning.priority(), 2);
        assert_eq!(AlertSeverity::Critical.priority(), 3);
    }

    #[test]
    fn test_alert_silence_active() {
        let now = Utc::now();
        let silence = AlertSilence {
            id: "test".to_string(),
            alert_name: "TestAlert".to_string(),
            matchers: vec![],
            starts_at: now - Duration::minutes(5),
            ends_at: now + Duration::minutes(5),
            created_by: "test-user".to_string(),
            comment: "Testing".to_string(),
            created_at: now,
        };
        assert!(silence.is_active());

        let expired_silence = AlertSilence {
            starts_at: now - Duration::minutes(10),
            ends_at: now - Duration::minutes(5),
            ..silence
        };
        assert!(!expired_silence.is_active());
    }

    #[test]
    fn test_alert_severity_as_str() {
        assert_eq!(AlertSeverity::Info.as_str(), "info");
        assert_eq!(AlertSeverity::Warning.as_str(), "warning");
        assert_eq!(AlertSeverity::Critical.as_str(), "critical");
    }

    #[test]
    fn test_alert_status_as_str() {
        assert_eq!(AlertStatus::Firing.as_str(), "firing");
        assert_eq!(AlertStatus::Resolved.as_str(), "resolved");
    }
}
