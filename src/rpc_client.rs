use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;
use std::sync::Arc;

use crate::models::{GetEventsResult, LatestLedgerResult, RpcResponse};

#[async_trait]
pub trait RpcClient: Send + Sync {
    async fn get_latest_ledger(&self, url: &str) -> Result<RpcResponse<LatestLedgerResult>, String>;
    async fn get_events(&self, url: &str, params: Value) -> Result<RpcResponse<GetEventsResult>, String>;
}

pub struct HttpRpcClient {
    client: Client,
}

impl HttpRpcClient {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl RpcClient for HttpRpcClient {
    async fn get_latest_ledger(&self, url: &str) -> Result<RpcResponse<LatestLedgerResult>, String> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLatestLedger"
        });

        self.client
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())
    }

    async fn get_events(&self, url: &str, params: Value) -> Result<RpcResponse<GetEventsResult>, String> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getEvents",
            "params": params
        });

        self.client
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
pub mod mock {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Debug, Clone)]
    pub struct MockRpcClient {
        responses: Arc<Mutex<HashMap<String, serde_json::Value>>>,
    }

    impl MockRpcClient {
        pub fn new() -> Self {
            Self {
                responses: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        pub fn with_responses(responses: HashMap<String, serde_json::Value>) -> Self {
            Self {
                responses: Arc::new(Mutex::new(responses)),
            }
        }

        pub fn set_response(&self, key: &str, response: serde_json::Value) {
            let mut responses = self.responses.lock().unwrap();
            responses.insert(key.to_string(), response);
        }
    }

    #[async_trait]
    impl RpcClient for MockRpcClient {
        async fn get_latest_ledger(&self, _url: &str) -> Result<RpcResponse<LatestLedgerResult>, String> {
            let responses = self.responses.lock().unwrap();
            if let Some(response) = responses.get("getLatestLedger") {
                serde_json::from_value(response.clone())
                    .map_err(|e| format!("Failed to deserialize mock response: {}", e))
            } else {
                Err("No mock response set for getLatestLedger".to_string())
            }
        }

        async fn get_events(&self, _url: &str, _params: Value) -> Result<RpcResponse<GetEventsResult>, String> {
            let responses = self.responses.lock().unwrap();
            if let Some(response) = responses.get("getEvents") {
                serde_json::from_value(response.clone())
                    .map_err(|e| format!("Failed to deserialize mock response: {}", e))
            } else {
                Err("No mock response set for getEvents".to_string())
            }
        }
    }
}
