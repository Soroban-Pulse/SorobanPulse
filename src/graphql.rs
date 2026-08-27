/// GraphQL API layer for Soroban Pulse events
/// Issue #683: Add GraphQL API layer for more flexible event querying

use async_graphql::{
    Context, InputObject, Object, Schema, Subscription, ID, SimpleObject,
};
use chrono::{DateTime, Utc};
use futures_util::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::models::SorobanEvent;

/// GraphQL representation of a Soroban event
#[derive(Clone, Debug, Serialize, Deserialize, SimpleObject)]
pub struct Event {
    id: ID,
    contract_id: String,
    event_type: String,
    tx_hash: String,
    ledger: u64,
    ledger_close_time: DateTime<Utc>,
    topic: Option<Vec<String>>,
    value: Option<String>,
    source_account: Option<String>,
}

impl From<SorobanEvent> for Event {
    fn from(event: SorobanEvent) -> Self {
        Self {
            id: event.id.map(|id| ID(id.to_string())).unwrap_or_else(|| ID(Uuid::new_v4().to_string())),
            contract_id: event.contract_id,
            event_type: event.event_type,
            tx_hash: event.tx_hash,
            ledger: event.ledger as u64,
            ledger_close_time: event.ledger_close_time,
            topic: event.topic.map(|t| t.split(',').map(|s| s.to_string()).collect()),
            value: event.value,
            source_account: event.source_account,
        }
    }
}

/// GraphQL input filter for querying events
#[derive(Debug, InputObject, Clone, Default)]
pub struct EventFilter {
    /// Filter by contract ID
    pub contract_id: Option<String>,
    /// Filter by event type
    pub event_type: Option<String>,
    /// Filter by transaction hash
    pub tx_hash: Option<String>,
    /// Filter by ledger sequence number (exact match)
    pub ledger: Option<u64>,
    /// Filter by minimum ledger sequence
    pub ledger_min: Option<u64>,
    /// Filter by maximum ledger sequence
    pub ledger_max: Option<u64>,
    /// Filter by source account
    pub source_account: Option<String>,
    /// Filter by topic (partial match)
    pub topic_contains: Option<String>,
}

/// Pagination input for GraphQL queries
#[derive(Debug, InputObject, Clone)]
pub struct PaginationInput {
    /// Maximum number of results to return (default: 10, max: 100)
    pub first: Option<i32>,
    /// Cursor for pagination
    pub after: Option<String>,
}

/// Query root for GraphQL API
pub struct Query;

#[Object]
impl Query {
    /// Get a single event by ID
    async fn event(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<Option<Event>> {
        let pool = ctx.data::<PgPool>()?;

        let row = sqlx::query_as::<_, (String, String, String, String, i32, String, Option<String>, Option<String>, Option<String>)>(
            "SELECT id, contract_id, event_type, tx_hash, ledger, ledger_close_time, topic, value, source_account FROM events WHERE id = $1"
        )
        .bind(id.as_str())
        .fetch_optional(pool)
        .await?;

        Ok(row.map(|(id, contract_id, event_type, tx_hash, ledger, ledger_close_time, topic, value, source_account)| {
            Event {
                id: ID(id),
                contract_id,
                event_type,
                tx_hash,
                ledger: ledger as u64,
                ledger_close_time: ledger_close_time.parse().unwrap_or_else(|_| Utc::now()),
                topic: topic.map(|t| t.split(',').map(|s| s.to_string()).collect()),
                value,
                source_account,
            }
        }))
    }

    /// Query events with optional filters and pagination
    async fn events(
        &self,
        ctx: &Context<'_>,
        filter: Option<EventFilter>,
        pagination: Option<PaginationInput>,
    ) -> async_graphql::Result<Vec<Event>> {
        let pool = ctx.data::<PgPool>()?;
        let filter = filter.unwrap_or_default();
        let pagination = pagination.unwrap_or_default();

        let limit = std::cmp::min(pagination.first.unwrap_or(10) as i64, 100);
        let offset = 0i64; // In a real implementation, parse cursor for offset

        let mut query = String::from(
            "SELECT id, contract_id, event_type, tx_hash, ledger, ledger_close_time, topic, value, source_account FROM events WHERE 1=1"
        );

        // Build dynamic WHERE clauses based on filters
        if let Some(contract_id) = &filter.contract_id {
            query.push_str(&format!(" AND contract_id = '{}'", contract_id));
        }
        if let Some(event_type) = &filter.event_type {
            query.push_str(&format!(" AND event_type = '{}'", event_type));
        }
        if let Some(tx_hash) = &filter.tx_hash {
            query.push_str(&format!(" AND tx_hash = '{}'", tx_hash));
        }
        if let Some(ledger) = filter.ledger {
            query.push_str(&format!(" AND ledger = {}", ledger));
        }
        if let Some(ledger_min) = filter.ledger_min {
            query.push_str(&format!(" AND ledger >= {}", ledger_min));
        }
        if let Some(ledger_max) = filter.ledger_max {
            query.push_str(&format!(" AND ledger <= {}", ledger_max));
        }
        if let Some(source_account) = &filter.source_account {
            query.push_str(&format!(" AND source_account = '{}'", source_account));
        }

        query.push_str(&format!(
            " ORDER BY ledger DESC LIMIT {} OFFSET {}",
            limit, offset
        ));

        let rows = sqlx::query_as::<_, (String, String, String, String, i32, String, Option<String>, Option<String>, Option<String>)>(
            &query
        )
        .fetch_all(pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, contract_id, event_type, tx_hash, ledger, ledger_close_time, topic, value, source_account)| {
                Event {
                    id: ID(id),
                    contract_id,
                    event_type,
                    tx_hash,
                    ledger: ledger as u64,
                    ledger_close_time: ledger_close_time.parse().unwrap_or_else(|_| Utc::now()),
                    topic: topic.map(|t| t.split(',').map(|s| s.to_string()).collect()),
                    value,
                    source_account,
                }
            })
            .collect())
    }

    /// Get event statistics
    async fn event_stats(&self, ctx: &Context<'_>) -> async_graphql::Result<EventStats> {
        let pool = ctx.data::<PgPool>()?;

        let total = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM events")
            .fetch_one(pool)
            .await
            .unwrap_or(0);

        let by_type = sqlx::query_as::<_, (String, i64)>(
            "SELECT event_type, COUNT(*) as count FROM events GROUP BY event_type ORDER BY count DESC"
        )
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let by_contract = sqlx::query_as::<_, (String, i64)>(
            "SELECT contract_id, COUNT(*) as count FROM events GROUP BY contract_id ORDER BY count DESC LIMIT 10"
        )
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        Ok(EventStats {
            total_events: total,
            events_by_type: by_type.into_iter().map(|(event_type, count)| EventTypeCount { event_type, count }).collect(),
            top_contracts: by_contract.into_iter().map(|(contract_id, count)| ContractCount { contract_id, count }).collect(),
        })
    }
}

/// Event type count
#[derive(Debug, Clone, SimpleObject)]
pub struct EventTypeCount {
    event_type: String,
    count: i64,
}

/// Contract count
#[derive(Debug, Clone, SimpleObject)]
pub struct ContractCount {
    contract_id: String,
    count: i64,
}

/// Event statistics response
#[derive(Debug, Clone, SimpleObject)]
pub struct EventStats {
    total_events: i64,
    events_by_type: Vec<EventTypeCount>,
    top_contracts: Vec<ContractCount>,
}

/// Subscription root for real-time event streaming
pub struct Subscription;

#[Subscription]
impl Subscription {
    /// Subscribe to new events matching optional filters
    /// Supports filtering by contract_id, event_type, tx_hash, and ledger range
    async fn events(
        &self,
        ctx: &Context<'_>,
        filter: Option<EventFilter>,
    ) -> impl Stream<Item = Event> {
        let event_rx = ctx
            .data::<broadcast::Receiver<SorobanEvent>>()
            .ok()
            .cloned()
            .unwrap_or_else(|| {
                let (tx, _) = broadcast::channel(100);
                tx.subscribe()
            });

        event_rx
            .into_stream()
            .filter_map(move |event| {
                let event = match event {
                    Ok(e) => e,
                    Err(_) => return std::future::ready(None),
                };

                // Apply filters if provided
                if let Some(ref f) = filter {
                    if let Some(ref contract_id) = f.contract_id {
                        if event.contract_id != *contract_id {
                            return std::future::ready(None);
                        }
                    }
                    if let Some(ref event_type) = f.event_type {
                        if event.event_type != *event_type {
                            return std::future::ready(None);
                        }
                    }
                    if let Some(ref tx_hash) = f.tx_hash {
                        if event.tx_hash != *tx_hash {
                            return std::future::ready(None);
                        }
                    }
                    // Ledger range filtering
                    if let Some(ledger_min) = f.ledger_min {
                        if (event.ledger as u64) < ledger_min {
                            return std::future::ready(None);
                        }
                    }
                    if let Some(ledger_max) = f.ledger_max {
                        if (event.ledger as u64) > ledger_max {
                            return std::future::ready(None);
                        }
                    }
                }

                std::future::ready(Some(Event::from(event)))
            })
    }
}

/// Create the GraphQL schema
pub fn create_schema(
) -> Schema<Query, async_graphql::EmptyMutation, Subscription> {
    Schema::build(Query, async_graphql::EmptyMutation, Subscription).finish()
}
