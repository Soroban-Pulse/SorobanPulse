use chrono::DateTime;
use reqwest::Client;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};

use crate::{
    config::Config,
    models::{GetEventsResult, RpcResponse, SorobanEvent},
};

pub struct Indexer {
    pool: PgPool,
    client: Client,
    config: Config,
}

impl Indexer {
    pub fn new(pool: PgPool, config: Config) -> Self {
        Self {
            pool,
            client: Client::new(),
            config,
        }
    }

    pub async fn run(&self) {
        let mut current_ledger = self.config.start_ledger;

        if current_ledger == 0 {
            current_ledger = self.get_latest_ledger().await.unwrap_or(1);
            info!("Starting from latest ledger: {}", current_ledger);
        }

        loop {
            match self.fetch_and_store_events(current_ledger).await {
                Ok(latest) => {
                    if latest > current_ledger {
                        current_ledger = latest;
                    } else {
                        sleep(Duration::from_secs(5)).await;
                    }
                }
                Err(e) => {
                    error!("Indexer error: {}", e);
                    sleep(Duration::from_secs(10)).await;
                }
            }
        }
    }

    async fn get_latest_ledger(&self) -> Result<u64, String> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLatestLedger"
        });

        let resp: Value = self
            .client
            .post(&self.config.stellar_rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;

        resp["result"]["sequence"]
            .as_u64()
            .ok_or_else(|| "Missing sequence".to_string())
    }

    async fn fetch_and_store_events(&self, start_ledger: u64) -> Result<u64, String> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getEvents",
            "params": {
                "startLedger": start_ledger,
                "filters": [],
                "pagination": { "limit": 100 }
            }
        });

        let resp: RpcResponse<GetEventsResult> = self
            .client
            .post(&self.config.stellar_rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;

        let result = match resp.result {
            Some(r) => r,
            None => return Ok(start_ledger),
        };

        let latest = result.latest_ledger;
        info!(
            "Fetched {} events up to ledger {}",
            result.events.len(),
            latest
        );

        for event in result.events {
            if let Err(e) = self.store_event(&event).await {
                warn!("Failed to store event {}: {}", event.tx_hash, e);
            }
        }

        Ok(latest + 1)
    }

    async fn store_event(&self, event: &SorobanEvent) -> Result<(), sqlx::Error> {
        let ledger = match i64::try_from(event.ledger) {
            Ok(v) => v,
            Err(_) => {
                error!(ledger = event.ledger, "Ledger number overflows i64, skipping event");
                return Ok(());
            }
        };

        let timestamp = DateTime::parse_from_rfc3339(&event.ledger_closed_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now());

        let event_data = json!({
            "value": event.value,
            "topic": event.topic
        });

        sqlx::query(
            r#"
            INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (tx_hash, contract_id, event_type) DO NOTHING
            "#,
        )
        .bind(&event.contract_id)
        .bind(&event.event_type)
        .bind(&event.tx_hash)
        .bind(ledger)
        .bind(timestamp)
        .bind(event_data)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn make_event(ledger: u64) -> SorobanEvent {
        SorobanEvent {
            contract_id: "C1".into(),
            event_type: "contract".into(),
            tx_hash: "abc".into(),
            ledger,
            ledger_closed_at: "2026-03-24T00:00:00Z".into(),
            value: Value::Null,
            topic: None,
        }
    }

    #[test]
    fn ledger_overflow_returns_err() {
        assert!(i64::try_from(make_event(u64::MAX).ledger).is_err());
    }
}
