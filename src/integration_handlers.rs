use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use tracing::{error, info};
use uuid::Uuid;

use crate::{
    discord::DiscordConfig,
    github::GitHubOAuthConfig,
    slack::SlackOAuthConfig,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct GitHubIntegrationRequest {
    pub access_token: String,
    pub owner: String,
    pub repository: String,
    pub issue_title_template: Option<String>,
    pub issue_body_template: Option<String>,
    pub auto_create_issues: Option<bool>,
    pub pr_comment_enabled: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscordIntegrationRequest {
    pub webhook_url: String,
    pub bot_name: Option<String>,
    pub avatar_url: Option<String>,
    pub embed_enabled: Option<bool>,
    pub thread_support: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SlackIntegrationRequest {
    pub webhook_url: Option<String>,
    pub bot_token: Option<String>,
    pub channel: String,
    pub block_kit_enabled: Option<bool>,
    pub thread_support: Option<bool>,
    pub user_mentions_enabled: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TelegramIntegrationRequest {
    pub bot_token: String,
    pub chat_id: String,
    pub webhook_enabled: Option<bool>,
    pub webhook_url: Option<String>,
    pub message_thread_support: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IntegrationResponse {
    pub id: Uuid,
    pub integration_type: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct GitHubOAuthCallback {
    pub code: String,
    pub state: String,
    pub subscription_id: String,
}

#[derive(Debug, Deserialize)]
pub struct SlackOAuthCallback {
    pub code: String,
    pub state: String,
    pub subscription_id: String,
}

/// Setup GitHub integration for a subscription
pub async fn setup_github_integration(
    State(pool): State<PgPool>,
    Path(subscription_id): Path<Uuid>,
    Json(req): Json<GitHubIntegrationRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let id = Uuid::new_v4();

    let result = sqlx::query(
        "INSERT INTO github_integrations (
            id, subscription_id, access_token, owner, repository,
            issue_title_template, issue_body_template, auto_create_issues, pr_comment_enabled
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (subscription_id) DO UPDATE SET
            access_token = EXCLUDED.access_token,
            owner = EXCLUDED.owner,
            repository = EXCLUDED.repository,
            issue_title_template = EXCLUDED.issue_title_template,
            issue_body_template = EXCLUDED.issue_body_template,
            auto_create_issues = EXCLUDED.auto_create_issues,
            pr_comment_enabled = EXCLUDED.pr_comment_enabled,
            updated_at = CURRENT_TIMESTAMP"
    )
    .bind(&id)
    .bind(&subscription_id)
    .bind(&req.access_token)
    .bind(&req.owner)
    .bind(&req.repository)
    .bind(&req.issue_title_template)
    .bind(&req.issue_body_template)
    .bind(req.auto_create_issues.unwrap_or(true))
    .bind(req.pr_comment_enabled.unwrap_or(false))
    .execute(&pool)
    .await;

    match result {
        Ok(_) => {
            info!(
                subscription_id = %subscription_id,
                owner = %req.owner,
                repository = %req.repository,
                "GitHub integration setup successful"
            );

            Ok((
                StatusCode::CREATED,
                Json(IntegrationResponse {
                    id,
                    integration_type: "github".to_string(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                }),
            ))
        }
        Err(e) => {
            error!(
                error = %e,
                subscription_id = %subscription_id,
                "Failed to setup GitHub integration"
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to setup GitHub integration: {}", e),
            ))
        }
    }
}

/// Setup Discord integration for a subscription
pub async fn setup_discord_integration(
    State(pool): State<PgPool>,
    Path(subscription_id): Path<Uuid>,
    Json(req): Json<DiscordIntegrationRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let id = Uuid::new_v4();

    let result = sqlx::query(
        "INSERT INTO discord_integrations (
            id, subscription_id, webhook_url, bot_name, avatar_url,
            embed_enabled, thread_support
        ) VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (subscription_id) DO UPDATE SET
            webhook_url = EXCLUDED.webhook_url,
            bot_name = EXCLUDED.bot_name,
            avatar_url = EXCLUDED.avatar_url,
            embed_enabled = EXCLUDED.embed_enabled,
            thread_support = EXCLUDED.thread_support,
            updated_at = CURRENT_TIMESTAMP"
    )
    .bind(&id)
    .bind(&subscription_id)
    .bind(&req.webhook_url)
    .bind(&req.bot_name)
    .bind(&req.avatar_url)
    .bind(req.embed_enabled.unwrap_or(true))
    .bind(req.thread_support.unwrap_or(false))
    .execute(&pool)
    .await;

    match result {
        Ok(_) => {
            info!(
                subscription_id = %subscription_id,
                webhook = %req.webhook_url,
                "Discord integration setup successful"
            );

            Ok((
                StatusCode::CREATED,
                Json(IntegrationResponse {
                    id,
                    integration_type: "discord".to_string(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                }),
            ))
        }
        Err(e) => {
            error!(
                error = %e,
                subscription_id = %subscription_id,
                "Failed to setup Discord integration"
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to setup Discord integration: {}", e),
            ))
        }
    }
}

/// Setup Slack integration for a subscription
pub async fn setup_slack_integration(
    State(pool): State<PgPool>,
    Path(subscription_id): Path<Uuid>,
    Json(req): Json<SlackIntegrationRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let id = Uuid::new_v4();

    let result = sqlx::query(
        "INSERT INTO slack_integrations (
            id, subscription_id, webhook_url, bot_token, channel,
            block_kit_enabled, thread_support, user_mentions_enabled
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (subscription_id) DO UPDATE SET
            webhook_url = EXCLUDED.webhook_url,
            bot_token = EXCLUDED.bot_token,
            channel = EXCLUDED.channel,
            block_kit_enabled = EXCLUDED.block_kit_enabled,
            thread_support = EXCLUDED.thread_support,
            user_mentions_enabled = EXCLUDED.user_mentions_enabled,
            updated_at = CURRENT_TIMESTAMP"
    )
    .bind(&id)
    .bind(&subscription_id)
    .bind(&req.webhook_url)
    .bind(&req.bot_token)
    .bind(&req.channel)
    .bind(req.block_kit_enabled.unwrap_or(true))
    .bind(req.thread_support.unwrap_or(false))
    .bind(req.user_mentions_enabled.unwrap_or(false))
    .execute(&pool)
    .await;

    match result {
        Ok(_) => {
            info!(
                subscription_id = %subscription_id,
                channel = %req.channel,
                "Slack integration setup successful"
            );

            Ok((
                StatusCode::CREATED,
                Json(IntegrationResponse {
                    id,
                    integration_type: "slack".to_string(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                }),
            ))
        }
        Err(e) => {
            error!(
                error = %e,
                subscription_id = %subscription_id,
                "Failed to setup Slack integration"
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to setup Slack integration: {}", e),
            ))
        }
    }
}

/// Setup Telegram integration for a subscription
pub async fn setup_telegram_integration(
    State(pool): State<PgPool>,
    Path(subscription_id): Path<Uuid>,
    Json(req): Json<TelegramIntegrationRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let id = Uuid::new_v4();

    let result = sqlx::query(
        "INSERT INTO telegram_integrations (
            id, subscription_id, bot_token, chat_id, webhook_enabled, webhook_url,
            message_thread_support, button_support
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (subscription_id) DO UPDATE SET
            bot_token = EXCLUDED.bot_token,
            chat_id = EXCLUDED.chat_id,
            webhook_enabled = EXCLUDED.webhook_enabled,
            webhook_url = EXCLUDED.webhook_url,
            message_thread_support = EXCLUDED.message_thread_support,
            button_support = EXCLUDED.button_support,
            updated_at = CURRENT_TIMESTAMP"
    )
    .bind(&id)
    .bind(&subscription_id)
    .bind(&req.bot_token)
    .bind(&req.chat_id)
    .bind(req.webhook_enabled.unwrap_or(false))
    .bind(&req.webhook_url)
    .bind(req.message_thread_support.unwrap_or(false))
    .bind(req.button_support.unwrap_or(true))
    .execute(&pool)
    .await;

    match result {
        Ok(_) => {
            info!(
                subscription_id = %subscription_id,
                chat_id = %req.chat_id,
                "Telegram integration setup successful"
            );

            Ok((
                StatusCode::CREATED,
                Json(IntegrationResponse {
                    id,
                    integration_type: "telegram".to_string(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                }),
            ))
        }
        Err(e) => {
            error!(
                error = %e,
                subscription_id = %subscription_id,
                "Failed to setup Telegram integration"
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to setup Telegram integration: {}", e),
            ))
        }
    }
}

/// Get GitHub integration details
pub async fn get_github_integration(
    State(pool): State<PgPool>,
    Path(subscription_id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let result = sqlx::query_as::<_, (Uuid, String, String, String)>(
        "SELECT id, owner, repository, webhook_url FROM github_integrations WHERE subscription_id = $1"
    )
    .bind(&subscription_id)
    .fetch_optional(&pool)
    .await;

    match result {
        Ok(Some((id, owner, repository, _))) => {
            Ok((
                StatusCode::OK,
                Json(json!({
                    "id": id,
                    "integration_type": "github",
                    "owner": owner,
                    "repository": repository,
                })),
            ))
        }
        Ok(None) => Err((StatusCode::NOT_FOUND, "Integration not found".to_string())),
        Err(e) => {
            error!(error = %e, "Failed to fetch GitHub integration");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch integration".to_string(),
            ))
        }
    }
}

/// Get Discord integration details
pub async fn get_discord_integration(
    State(pool): State<PgPool>,
    Path(subscription_id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let result = sqlx::query_as::<_, (Uuid, String, Option<String>)>(
        "SELECT id, webhook_url, bot_name FROM discord_integrations WHERE subscription_id = $1"
    )
    .bind(&subscription_id)
    .fetch_optional(&pool)
    .await;

    match result {
        Ok(Some((id, webhook_url, bot_name))) => {
            Ok((
                StatusCode::OK,
                Json(json!({
                    "id": id,
                    "integration_type": "discord",
                    "webhook_url": webhook_url,
                    "bot_name": bot_name,
                })),
            ))
        }
        Ok(None) => Err((StatusCode::NOT_FOUND, "Integration not found".to_string())),
        Err(e) => {
            error!(error = %e, "Failed to fetch Discord integration");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch integration".to_string(),
            ))
        }
    }
}

/// Get Slack integration details
pub async fn get_slack_integration(
    State(pool): State<PgPool>,
    Path(subscription_id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let result = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, channel FROM slack_integrations WHERE subscription_id = $1"
    )
    .bind(&subscription_id)
    .fetch_optional(&pool)
    .await;

    match result {
        Ok(Some((id, channel))) => {
            Ok((
                StatusCode::OK,
                Json(json!({
                    "id": id,
                    "integration_type": "slack",
                    "channel": channel,
                })),
            ))
        }
        Ok(None) => Err((StatusCode::NOT_FOUND, "Integration not found".to_string())),
        Err(e) => {
            error!(error = %e, "Failed to fetch Slack integration");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch integration".to_string(),
            ))
        }
    }
}

/// Get Telegram integration details
pub async fn get_telegram_integration(
    State(pool): State<PgPool>,
    Path(subscription_id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let result = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, chat_id FROM telegram_integrations WHERE subscription_id = $1"
    )
    .bind(&subscription_id)
    .fetch_optional(&pool)
    .await;

    match result {
        Ok(Some((id, chat_id))) => {
            Ok((
                StatusCode::OK,
                Json(json!({
                    "id": id,
                    "integration_type": "telegram",
                    "chat_id": chat_id,
                })),
            ))
        }
        Ok(None) => Err((StatusCode::NOT_FOUND, "Integration not found".to_string())),
        Err(e) => {
            error!(error = %e, "Failed to fetch Telegram integration");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch integration".to_string(),
            ))
        }
    }
}

/// Delete GitHub integration
pub async fn delete_github_integration(
    State(pool): State<PgPool>,
    Path(subscription_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let result = sqlx::query("DELETE FROM github_integrations WHERE subscription_id = $1")
        .bind(&subscription_id)
        .execute(&pool)
        .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => {
            info!(subscription_id = %subscription_id, "GitHub integration deleted");
            Ok(StatusCode::NO_CONTENT)
        }
        Ok(_) => Err((StatusCode::NOT_FOUND, "Integration not found".to_string())),
        Err(e) => {
            error!(error = %e, "Failed to delete GitHub integration");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to delete integration".to_string(),
            ))
        }
    }
}

/// Delete Discord integration
pub async fn delete_discord_integration(
    State(pool): State<PgPool>,
    Path(subscription_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let result = sqlx::query("DELETE FROM discord_integrations WHERE subscription_id = $1")
        .bind(&subscription_id)
        .execute(&pool)
        .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => {
            info!(subscription_id = %subscription_id, "Discord integration deleted");
            Ok(StatusCode::NO_CONTENT)
        }
        Ok(_) => Err((StatusCode::NOT_FOUND, "Integration not found".to_string())),
        Err(e) => {
            error!(error = %e, "Failed to delete Discord integration");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to delete integration".to_string(),
            ))
        }
    }
}

/// Delete Slack integration
pub async fn delete_slack_integration(
    State(pool): State<PgPool>,
    Path(subscription_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let result = sqlx::query("DELETE FROM slack_integrations WHERE subscription_id = $1")
        .bind(&subscription_id)
        .execute(&pool)
        .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => {
            info!(subscription_id = %subscription_id, "Slack integration deleted");
            Ok(StatusCode::NO_CONTENT)
        }
        Ok(_) => Err((StatusCode::NOT_FOUND, "Integration not found".to_string())),
        Err(e) => {
            error!(error = %e, "Failed to delete Slack integration");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to delete integration".to_string(),
            ))
        }
    }
}

/// Delete Telegram integration
pub async fn delete_telegram_integration(
    State(pool): State<PgPool>,
    Path(subscription_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let result = sqlx::query("DELETE FROM telegram_integrations WHERE subscription_id = $1")
        .bind(&subscription_id)
        .execute(&pool)
        .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => {
            info!(subscription_id = %subscription_id, "Telegram integration deleted");
            Ok(StatusCode::NO_CONTENT)
        }
        Ok(_) => Err((StatusCode::NOT_FOUND, "Integration not found".to_string())),
        Err(e) => {
            error!(error = %e, "Failed to delete Telegram integration");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to delete integration".to_string(),
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// PagerDuty integration — Issue #951
// ---------------------------------------------------------------------------

/// Request body for creating or updating a PagerDuty integration.
#[derive(Debug, Serialize, Deserialize)]
pub struct PagerDutyIntegrationRequest {
    /// Events API v2 routing key (required).
    pub routing_key: String,
    /// Human-readable service name shown in incidents.
    pub service_name: Option<String>,
    /// REST API key used for schedule / escalation policy lookups (optional).
    pub api_key: Option<String>,
    /// PagerDuty escalation policy ID to attach to new incidents (optional).
    pub escalation_policy_id: Option<String>,
    /// Contract IDs that should trigger incidents. Empty list = all.
    pub contract_filter: Option<Vec<String>>,
    /// Event types that trigger incidents. Empty list = all.
    pub event_type_filter: Option<Vec<String>>,
    /// JSON object mapping event type → severity level.
    pub severity_mapping: Option<serde_json::Value>,
    /// Auto-resolve stale open incidents.
    pub auto_resolve: Option<bool>,
    /// Minutes before an incident without new events is auto-resolved.
    pub auto_resolve_threshold_min: Option<i32>,
}

/// Request body for acknowledging an incident.
#[derive(Debug, Deserialize)]
pub struct AcknowledgeIncidentRequest {
    pub dedup_key: String,
    pub acknowledged_by: Option<String>,
}

/// Request body for resolving an incident.
#[derive(Debug, Deserialize)]
pub struct ResolveIncidentRequest {
    pub dedup_key: String,
}

/// Setup (create or update) a PagerDuty integration for a subscription.
pub async fn setup_pagerduty_integration(
    State(pool): State<PgPool>,
    Path(subscription_id): Path<Uuid>,
    Json(req): Json<PagerDutyIntegrationRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let id = Uuid::new_v4();

    let service_name = req.service_name.unwrap_or_else(|| "Soroban Pulse".to_string());
    let contract_filter: Vec<String> = req.contract_filter.unwrap_or_default();
    let event_type_filter: Vec<String> = req.event_type_filter.unwrap_or_default();
    let severity_mapping = req.severity_mapping.unwrap_or_else(|| {
        serde_json::json!({"contract": "error", "diagnostic": "warning", "system": "info"})
    });
    let auto_resolve = req.auto_resolve.unwrap_or(true);
    let auto_resolve_threshold_min = req.auto_resolve_threshold_min.unwrap_or(30);

    let result = sqlx::query(
        "INSERT INTO pagerduty_integrations (
             id, subscription_id, routing_key, service_name, api_key,
             escalation_policy_id, contract_filter, event_type_filter,
             severity_mapping, auto_resolve, auto_resolve_threshold_min
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
         ON CONFLICT (subscription_id) DO UPDATE SET
             routing_key              = EXCLUDED.routing_key,
             service_name             = EXCLUDED.service_name,
             api_key                  = EXCLUDED.api_key,
             escalation_policy_id     = EXCLUDED.escalation_policy_id,
             contract_filter          = EXCLUDED.contract_filter,
             event_type_filter        = EXCLUDED.event_type_filter,
             severity_mapping         = EXCLUDED.severity_mapping,
             auto_resolve             = EXCLUDED.auto_resolve,
             auto_resolve_threshold_min = EXCLUDED.auto_resolve_threshold_min,
             updated_at               = CURRENT_TIMESTAMP",
    )
    .bind(&id)
    .bind(&subscription_id)
    .bind(&req.routing_key)
    .bind(&service_name)
    .bind(&req.api_key)
    .bind(&req.escalation_policy_id)
    .bind(&contract_filter)
    .bind(&event_type_filter)
    .bind(&severity_mapping)
    .bind(auto_resolve)
    .bind(auto_resolve_threshold_min)
    .execute(&pool)
    .await;

    match result {
        Ok(_) => {
            info!(
                subscription_id = %subscription_id,
                service_name    = %service_name,
                "PagerDuty integration configured"
            );
            Ok((
                StatusCode::CREATED,
                Json(IntegrationResponse {
                    id,
                    integration_type: "pagerduty".to_string(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                }),
            ))
        }
        Err(e) => {
            error!(error = %e, subscription_id = %subscription_id, "Failed to configure PagerDuty integration");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to configure PagerDuty integration: {}", e),
            ))
        }
    }
}

/// Retrieve PagerDuty integration details for a subscription.
pub async fn get_pagerduty_integration(
    State(pool): State<PgPool>,
    Path(subscription_id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let result = sqlx::query_as::<_, (Uuid, String, Option<String>, bool, i32)>(
        "SELECT id, service_name, escalation_policy_id, auto_resolve, auto_resolve_threshold_min
         FROM pagerduty_integrations
         WHERE subscription_id = $1",
    )
    .bind(&subscription_id)
    .fetch_optional(&pool)
    .await;

    match result {
        Ok(Some((id, service_name, escalation_policy_id, auto_resolve, threshold))) => Ok((
            StatusCode::OK,
            Json(json!({
                "id":                       id,
                "integration_type":         "pagerduty",
                "service_name":             service_name,
                "escalation_policy_id":     escalation_policy_id,
                "auto_resolve":             auto_resolve,
                "auto_resolve_threshold_min": threshold,
            })),
        )),
        Ok(None) => Err((StatusCode::NOT_FOUND, "PagerDuty integration not found".to_string())),
        Err(e) => {
            error!(error = %e, "Failed to fetch PagerDuty integration");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch integration".to_string(),
            ))
        }
    }
}

/// Delete the PagerDuty integration for a subscription.
pub async fn delete_pagerduty_integration(
    State(pool): State<PgPool>,
    Path(subscription_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let result =
        sqlx::query("DELETE FROM pagerduty_integrations WHERE subscription_id = $1")
            .bind(&subscription_id)
            .execute(&pool)
            .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => {
            info!(subscription_id = %subscription_id, "PagerDuty integration deleted");
            Ok(StatusCode::NO_CONTENT)
        }
        Ok(_) => Err((StatusCode::NOT_FOUND, "Integration not found".to_string())),
        Err(e) => {
            error!(error = %e, "Failed to delete PagerDuty integration");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to delete integration".to_string(),
            ))
        }
    }
}

/// Acknowledge an open incident via the PagerDuty Events API.
///
/// The routing key is resolved from the subscription's integration row.
pub async fn acknowledge_pagerduty_incident(
    State(pool): State<PgPool>,
    Path(subscription_id): Path<Uuid>,
    Json(req): Json<AcknowledgeIncidentRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Fetch routing key from the integration row
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT routing_key FROM pagerduty_integrations WHERE subscription_id = $1",
    )
    .bind(&subscription_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to fetch PagerDuty routing key");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to fetch integration".to_string(),
        )
    })?;

    let (routing_key,) = row.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            "PagerDuty integration not configured for this subscription".to_string(),
        )
    })?;

    let mut config = crate::pagerduty::PagerDutyConfig::default();
    config.routing_key = routing_key;
    let client = crate::pagerduty::PagerDutyClient::new(config);

    client
        .acknowledge_incident(
            &req.dedup_key,
            req.acknowledged_by.as_deref(),
            Some(&pool),
        )
        .await
        .map_err(|e| {
            error!(error = %e, dedup_key = %req.dedup_key, "Acknowledge failed");
            (
                StatusCode::BAD_GATEWAY,
                format!("PagerDuty acknowledge failed: {}", e),
            )
        })?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "status":    "acknowledged",
            "dedup_key": req.dedup_key,
        })),
    ))
}

/// Resolve an incident via the PagerDuty Events API.
pub async fn resolve_pagerduty_incident(
    State(pool): State<PgPool>,
    Path(subscription_id): Path<Uuid>,
    Json(req): Json<ResolveIncidentRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT routing_key FROM pagerduty_integrations WHERE subscription_id = $1",
    )
    .bind(&subscription_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to fetch PagerDuty routing key");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to fetch integration".to_string(),
        )
    })?;

    let (routing_key,) = row.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            "PagerDuty integration not configured for this subscription".to_string(),
        )
    })?;

    let mut config = crate::pagerduty::PagerDutyConfig::default();
    config.routing_key = routing_key;
    let client = crate::pagerduty::PagerDutyClient::new(config);

    client
        .resolve_incident(&req.dedup_key, Some(&pool))
        .await
        .map_err(|e| {
            error!(error = %e, dedup_key = %req.dedup_key, "Resolve failed");
            (
                StatusCode::BAD_GATEWAY,
                format!("PagerDuty resolve failed: {}", e),
            )
        })?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "status":    "resolved",
            "dedup_key": req.dedup_key,
        })),
    ))
}

/// List open incidents for a subscription.
pub async fn list_pagerduty_incidents(
    State(pool): State<PgPool>,
    Path(subscription_id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let incidents = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            Option<String>,
            String,
            String,
            String,
            Option<String>,
            chrono::DateTime<chrono::Utc>,
        ),
    >(
        "SELECT pi.id, pi.dedup_key, pi.incident_key, pi.contract_id,
                pi.event_type, pi.status, pi.acknowledged_by, pi.created_at
         FROM pagerduty_incidents pi
         JOIN pagerduty_integrations pdi ON pdi.id = pi.integration_id
         WHERE pdi.subscription_id = $1
         ORDER BY pi.created_at DESC
         LIMIT 100",
    )
    .bind(&subscription_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to list PagerDuty incidents");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to list incidents".to_string(),
        )
    })?;

    let items: Vec<serde_json::Value> = incidents
        .into_iter()
        .map(
            |(id, dedup_key, incident_key, contract_id, event_type, status, acked_by, created_at)| {
                json!({
                    "id":              id,
                    "dedup_key":       dedup_key,
                    "incident_key":    incident_key,
                    "contract_id":     contract_id,
                    "event_type":      event_type,
                    "status":          status,
                    "acknowledged_by": acked_by,
                    "created_at":      created_at,
                })
            },
        )
        .collect();

    Ok((StatusCode::OK, Json(json!({ "incidents": items }))))
}
