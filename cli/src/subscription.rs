//! Subscription management — Issue #964
//!
//! Thin wrappers around the `/subscriptions` REST endpoints so subscriptions
//! can be created, inspected, acknowledged, and torn down from the command
//! line instead of curl.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::ApiClient;

// ---------------------------------------------------------------------------
// Request/response shapes (mirrors src/subscriptions.rs on the server)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct CreateSubscriptionRequest {
    pub callback_url: String,
    pub from_ledger: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscription_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_size: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_timeout_ms: Option<i32>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Subscription {
    pub id: String,
    pub callback_url: String,
    pub from_ledger: i64,
    pub acked_ledger: i64,
    pub status: String,
    pub created_at: String,
    #[serde(default)]
    pub subscription_type: Option<String>,
}

#[derive(Debug, Serialize)]
struct AckRequest {
    ledger: i64,
}

#[derive(Debug, Serialize)]
struct PauseRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pause_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct ResumeRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

pub fn create(
    client: &ApiClient,
    callback_url: String,
    from_ledger: i64,
    subscription_type: Option<String>,
    batch_size: Option<i32>,
    batch_timeout_ms: Option<i32>,
) -> anyhow::Result<Subscription> {
    let req = CreateSubscriptionRequest {
        callback_url,
        from_ledger,
        subscription_type,
        batch_size,
        batch_timeout_ms,
    };
    client.post("/subscriptions", &req)
}

pub fn get(client: &ApiClient, id: &str) -> anyhow::Result<Subscription> {
    client.get(&format!("/subscriptions/{id}"), &[])
}

pub fn delete(client: &ApiClient, id: &str) -> anyhow::Result<()> {
    client.delete(&format!("/subscriptions/{id}"))
}

pub fn ack(client: &ApiClient, id: &str, ledger: i64) -> anyhow::Result<Value> {
    client.post(&format!("/subscriptions/{id}/ack"), &AckRequest { ledger })
}

pub fn pause(client: &ApiClient, id: &str, pause_seconds: Option<i64>, reason: Option<String>) -> anyhow::Result<Value> {
    client.post(
        &format!("/subscriptions/{id}/pause"),
        &PauseRequest { pause_seconds, reason },
    )
}

pub fn resume(client: &ApiClient, id: &str, reason: Option<String>) -> anyhow::Result<Value> {
    client.post(&format!("/subscriptions/{id}/resume"), &ResumeRequest { reason })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_request_omits_unset_optional_fields() {
        let req = CreateSubscriptionRequest {
            callback_url: "https://example.com/hook".into(),
            from_ledger: 42,
            subscription_type: None,
            batch_size: None,
            batch_timeout_ms: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "callback_url": "https://example.com/hook", "from_ledger": 42 })
        );
    }

    #[test]
    fn create_request_includes_batch_fields_when_set() {
        let req = CreateSubscriptionRequest {
            callback_url: "https://example.com/hook".into(),
            from_ledger: 42,
            subscription_type: Some("batch".into()),
            batch_size: Some(50),
            batch_timeout_ms: Some(5000),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["subscription_type"], "batch");
        assert_eq!(json["batch_size"], 50);
        assert_eq!(json["batch_timeout_ms"], 5000);
    }

    #[test]
    fn subscription_response_deserializes_without_optional_type() {
        let raw = serde_json::json!({
            "id": "018f2e6a-0000-4000-8000-000000000000",
            "callback_url": "https://example.com/hook",
            "from_ledger": 1,
            "acked_ledger": 0,
            "status": "active",
            "created_at": "2026-01-01T00:00:00Z"
        });
        let sub: Subscription = serde_json::from_value(raw).unwrap();
        assert_eq!(sub.status, "active");
        assert!(sub.subscription_type.is_none());
    }
}
