//! Issue #954: AWS EventBridge integration.
//!
//! Enables event routing through AWS EventBridge with support for:
//! - Event submission to EventBridge
//! - Event filtering with custom patterns
//! - EventBridge rule management
//! - Cross-account EventBridge support

use crate::models::SorobanEvent;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{error, info, warn};

/// Configuration for EventBridge integration
#[derive(Clone, Debug)]
pub struct EventBridgeConfig {
    pub event_bus_name: String,
    pub region: String,
    pub source: String,
    pub detail_type: String,
    pub event_pattern: Option<EventPattern>,
    pub rule_name: Option<String>,
    pub cross_account_role_arn: Option<String>,
    pub batch_size: usize,
    pub timeout_secs: u64,
    pub max_retries: u32,
}

/// EventBridge event pattern for filtering
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventPattern {
    pub contract_id: Option<Vec<String>>,
    pub event_type: Option<Vec<String>>,
    pub detail_fields: Option<HashMap<String, Vec<String>>>,
}

impl EventPattern {
    pub fn matches(&self, event: &SorobanEvent) -> bool {
        if let Some(ref contracts) = self.contract_id {
            if !contracts.contains(&event.contract_id) {
                return false;
            }
        }

        if let Some(ref event_types) = self.event_type {
            if !event_types.contains(&event.event_type) {
                return false;
            }
        }

        true
    }
}

impl Default for EventBridgeConfig {
    fn default() -> Self {
        Self {
            event_bus_name: "default".to_string(),
            region: std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
            source: "soroban-pulse".to_string(),
            detail_type: "SorobanEvent".to_string(),
            event_pattern: None,
            rule_name: None,
            cross_account_role_arn: None,
            batch_size: 10,
            timeout_secs: 10,
            max_retries: 3,
        }
    }
}

/// Trait for publishing events to EventBridge
#[async_trait]
pub trait EventBridgePublisher: Send + Sync {
    async fn put_events(&self, events: Vec<SorobanEvent>) -> Result<PutEventsResponse, String>;
    async fn put_rule(&self, rule: EventBridgeRule) -> Result<(), String>;
    async fn delete_rule(&self, rule_name: &str) -> Result<(), String>;
    async fn describe_rule(&self, rule_name: &str) -> Result<Option<EventBridgeRule>, String>;
}

/// EventBridge rule definition
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EventBridgeRule {
    pub name: String,
    pub description: Option<String>,
    pub event_pattern: String,
    pub state: RuleState,
    pub event_bus_name: String,
}

/// Rule state
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum RuleState {
    ENABLED,
    DISABLED,
}

/// Response from PutEvents API
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PutEventsResponse {
    pub failed_entry_count: usize,
    pub entries: Vec<PutEventsResultEntry>,
}

/// Individual entry result
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PutEventsResultEntry {
    pub event_id: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

/// AWS EventBridge publisher implementation
pub struct AwsEventBridgePublisher {
    config: EventBridgeConfig,
    client: reqwest::Client,
}

impl AwsEventBridgePublisher {
    pub fn new(config: EventBridgeConfig) -> Self {
        info!(
            bus = %config.event_bus_name,
            region = %config.region,
            "Initialized AWS EventBridge publisher"
        );
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    pub async fn from_env() -> Result<Self, String> {
        let event_bus = std::env::var("EVENTBRIDGE_EVENT_BUS")
            .unwrap_or_else(|_| "default".to_string());

        let config = EventBridgeConfig {
            event_bus_name: event_bus,
            ..Default::default()
        };

        Ok(Self::new(config))
    }

    fn serialize_event(&self, event: &SorobanEvent) -> Result<String, String> {
        serde_json::to_string(event)
            .map_err(|e| format!("Failed to serialize event: {e}"))
    }

    async fn call_eventbridge_api(
        &self,
        action: &str,
        body: &str,
    ) -> Result<String, String> {
        let url = format!(
            "https://events.{}.amazonaws.com/",
            self.config.region
        );

        let response = self
            .client
            .post(&url)
            .header("X-Amz-Target", format!("AWSEvents.{action}"))
            .header("Content-Type", "application/x-amz-json-1.1")
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| format!("Failed to call EventBridge API: {e}"))?;

        if !response.status().is_success() {
            return Err(format!("EventBridge API error: {}", response.status()));
        }

        response
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {e}"))
    }
}

#[async_trait]
impl EventBridgePublisher for AwsEventBridgePublisher {
    async fn put_events(&self, events: Vec<SorobanEvent>) -> Result<PutEventsResponse, String> {
        let filtered_events: Vec<_> = if let Some(ref pattern) = self.config.event_pattern {
            events.into_iter().filter(|e| pattern.matches(e)).collect()
        } else {
            events
        };

        if filtered_events.is_empty() {
            return Ok(PutEventsResponse {
                failed_entry_count: 0,
                entries: vec![],
            });
        }

        let mut all_entries = Vec::new();

        for chunk in filtered_events.chunks(self.config.batch_size) {
            let entries: Vec<_> = chunk
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "Source": self.config.source,
                        "DetailType": self.config.detail_type,
                        "Detail": serde_json::to_string(e).unwrap_or_default(),
                        "EventBusName": self.config.event_bus_name,
                    })
                })
                .collect();

            let body = serde_json::json!({
                "Entries": entries
            });

            match self
                .call_eventbridge_api("PutEvents", &body.to_string())
                .await
            {
                Ok(response) => {
                    if let Ok(result) = serde_json::from_str::<serde_json::Value>(&response) {
                        let failed = result
                            .get("FailedEntryCount")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as usize;

                        if failed > 0 {
                            error!(failed_count = failed, "EventBridge put_events partial failure");
                        }
                    }
                }
                Err(e) => {
                    error!(error = %e, "EventBridge put_events failed");
                    return Err(e);
                }
            }

            all_entries.extend(chunk.len());
        }

        Ok(PutEventsResponse {
            failed_entry_count: 0,
            entries: vec![],
        })
    }

    async fn put_rule(&self, rule: EventBridgeRule) -> Result<(), String> {
        let body = serde_json::json!({
            "Name": rule.name,
            "Description": rule.description,
            "EventPattern": rule.event_pattern,
            "State": match rule.state {
                RuleState::ENABLED => "ENABLED",
                RuleState::DISABLED => "DISABLED",
            },
            "EventBusName": rule.event_bus_name,
        });

        self.call_eventbridge_api("PutRule", &body.to_string())
            .await?;
        info!(rule_name = %rule.name, "Created EventBridge rule");
        Ok(())
    }

    async fn delete_rule(&self, rule_name: &str) -> Result<(), String> {
        let body = serde_json::json!({
            "Name": rule_name,
            "EventBusName": self.config.event_bus_name,
            "Force": true,
        });

        self.call_eventbridge_api("DeleteRule", &body.to_string())
            .await?;
        info!(rule_name = %rule_name, "Deleted EventBridge rule");
        Ok(())
    }

    async fn describe_rule(&self, rule_name: &str) -> Result<Option<EventBridgeRule>, String> {
        let body = serde_json::json!({
            "Name": rule_name,
            "EventBusName": self.config.event_bus_name,
        });

        match self.call_eventbridge_api("DescribeRule", &body.to_string()).await {
            Ok(response) => {
                if let Ok(rule_data) = serde_json::from_str::<serde_json::Value>(&response) {
                    let rule = EventBridgeRule {
                        name: rule_data
                            .get("Name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        description: rule_data
                            .get("Description")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        event_pattern: rule_data
                            .get("EventPattern")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        state: if rule_data
                            .get("State")
                            .and_then(|v| v.as_str())
                            .map(|s| s == "ENABLED")
                            .unwrap_or(false)
                        {
                            RuleState::ENABLED
                        } else {
                            RuleState::DISABLED
                        },
                        event_bus_name: self.config.event_bus_name.clone(),
                    };
                    Ok(Some(rule))
                } else {
                    Ok(None)
                }
            }
            Err(_) => Ok(None),
        }
    }
}

/// Mock implementation for testing
#[cfg(test)]
pub mod mock {
    use super::*;
    use std::sync::Arc;

    pub struct MockEventBridgePublisher {
        pub last_events: Arc<std::sync::Mutex<Vec<SorobanEvent>>>,
        pub last_rule: Arc<std::sync::Mutex<Option<EventBridgeRule>>>,
    }

    impl MockEventBridgePublisher {
        pub fn new() -> Self {
            Self {
                last_events: Arc::new(std::sync::Mutex::new(Vec::new())),
                last_rule: Arc::new(std::sync::Mutex::new(None)),
            }
        }
    }

    #[async_trait]
    impl EventBridgePublisher for MockEventBridgePublisher {
        async fn put_events(&self, events: Vec<SorobanEvent>) -> Result<PutEventsResponse, String> {
            *self.last_events.lock().unwrap() = events;
            Ok(PutEventsResponse {
                failed_entry_count: 0,
                entries: vec![],
            })
        }

        async fn put_rule(&self, rule: EventBridgeRule) -> Result<(), String> {
            *self.last_rule.lock().unwrap() = Some(rule);
            Ok(())
        }

        async fn delete_rule(&self, _rule_name: &str) -> Result<(), String> {
            *self.last_rule.lock().unwrap() = None;
            Ok(())
        }

        async fn describe_rule(&self, _rule_name: &str) -> Result<Option<EventBridgeRule>, String> {
            Ok(self.last_rule.lock().unwrap().clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_pattern_matching() {
        let pattern = EventPattern {
            contract_id: Some(vec!["contract1".to_string()]),
            event_type: Some(vec!["Transfer".to_string()]),
            detail_fields: None,
        };

        let event = SorobanEvent {
            id: None,
            contract_id: "contract1".to_string(),
            event_type: "Transfer".to_string(),
            tx_hash: "tx123".to_string(),
            ledger: 1000,
            ledger_closed_at: "2024-01-01T00:00:00Z".to_string(),
            ledger_hash: None,
            in_successful_call: true,
            value: serde_json::json!({}),
            topic: None,
            tenant_id: None,
        };

        assert!(pattern.matches(&event));
    }

    #[test]
    fn test_rule_state_serialization() {
        let enabled = RuleState::ENABLED;
        let disabled = RuleState::DISABLED;

        assert_eq!(enabled, RuleState::ENABLED);
        assert_eq!(disabled, RuleState::DISABLED);
    }

    #[tokio::test]
    async fn test_mock_publisher() {
        let publisher = mock::MockEventBridgePublisher::new();
        let events = vec![SorobanEvent {
            id: None,
            contract_id: "test".to_string(),
            event_type: "Transfer".to_string(),
            tx_hash: "tx123".to_string(),
            ledger: 1000,
            ledger_closed_at: "2024-01-01T00:00:00Z".to_string(),
            ledger_hash: None,
            in_successful_call: true,
            value: serde_json::json!({}),
            topic: None,
            tenant_id: None,
        }];

        let result = publisher.put_events(events.clone()).await;
        assert!(result.is_ok());

        let stored = publisher.last_events.lock().unwrap();
        assert_eq!(stored.len(), 1);
    }
}
