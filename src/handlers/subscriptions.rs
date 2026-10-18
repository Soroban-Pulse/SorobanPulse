//! Subscription and notification-channel handlers: email unsubscribe,
//! open/click tracking, A/B results, suppression lists, notification
//! templates, dashboards and channel management.

use super::*;

/// Issue #480: List the available multi-language email notification templates.
///
/// `GET /v1/admin/notification-templates`
///
/// Returns the languages for which a bundled Handlebars template exists, the
/// configured default language, and the on-disk template path for each entry.
#[utoipa::path(
    get,
    path = "/v1/admin/notification-templates",
    tag = "admin",
    responses(
        (status = 200, description = "List of available notification templates")
    )
)]
pub async fn list_notification_templates() -> impl IntoResponse {
    let templates: Vec<Value> = crate::email::SUPPORTED_LANGUAGES
        .iter()
        .map(|lang| {
            json!({
                "language": lang,
                "engine": "handlebars",
                "format": "text",
                "file": format!("notification_templates/email_{lang}.hbs"),
            })
        })
        .collect();

    Json(json!({
        "default_language": "en",
        "count": templates.len(),
        "templates": templates,
    }))
}

/// Query parameters for the email unsubscribe endpoint (Issue #483).
#[derive(serde::Deserialize)]
pub struct UnsubscribeQuery {
    pub token: String,
}

/// Public, unauthenticated endpoint that recipients reach from the
/// "unsubscribe" link in notification emails (Issue #483, CAN-SPAM/GDPR).
/// Marks the token's recipient as opted out and returns a small HTML page.
#[utoipa::path(
    get,
    path = "/unsubscribe",
    tag = "system",
    params(("token" = String, Query, description = "Per-recipient unsubscribe token")),
    responses(
        (status = 200, description = "Unsubscribed (or already unsubscribed)"),
        (status = 404, description = "Unknown unsubscribe token"),
    )
)]
pub async fn unsubscribe(
    State(state): State<AppState>,
    Query(query): Query<UnsubscribeQuery>,
) -> Response {
    fn html_page(status: StatusCode, title: &str, message: &str) -> Response {
        let body = format!(
            "<!DOCTYPE html><html><head><meta charset=\"utf-8\">\
             <title>{title}</title></head><body style=\"font-family:sans-serif;\
             max-width:32rem;margin:4rem auto;text-align:center;\">\
             <h1>{title}</h1><p>{message}</p></body></html>"
        );
        Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(body))
            .expect("static html response is always valid")
    }

    // Verify the HMAC-signed, time-limited token (issue #1158).
    // The subject carried in the token is the opaque DB token string that
    // `mark_unsubscribed` looks up; we verify purpose = Unsubscribe before
    // touching the DB.
    let db_token = match crate::email_token::verify_token(
        &query.token,
        Some(&crate::email_token::TokenPurpose::Unsubscribe),
        None,
    ) {
        Ok(claims) => claims.subject,
        Err(_) => {
            // Return 404 (not 400) to avoid leaking why the token failed.
            return html_page(
                StatusCode::NOT_FOUND,
                "Invalid link",
                "This unsubscribe link is not valid.",
            );
        }
    };

    match crate::email::mark_unsubscribed(&state.pool, &db_token).await {
        Ok(true) => html_page(
            StatusCode::OK,
            "Unsubscribed",
            "You have been unsubscribed from Soroban Pulse notifications.",
        ),
        Ok(false) => html_page(
            StatusCode::NOT_FOUND,
            "Invalid link",
            "This unsubscribe link is not valid.",
        ),
        Err(e) => {
            tracing::error!(error = %e, "Failed to process unsubscribe request");
            html_page(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Something went wrong",
                "We could not process your request. Please try again later.",
            )
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// #499 – Notification channel failover
// #500 – Notification analytics dashboard
// #501 – Notification cost tracking
// #502 – Notification channel testing mode
// ─────────────────────────────────────────────────────────────────────────────

/// GET /v1/admin/notifications/dashboard
///
/// Returns a comprehensive analytics summary: totals per window, per-channel delivery
/// success rates, average delivery latency, and the top-10 most-notified contracts.
pub async fn notification_dashboard(
    State(state): State<AppState>,
) -> impl IntoResponse {
    // Total notifications sent in each time window
    let windows: &[(&str, &str)] = &[
        ("last_24h", "NOW() - INTERVAL '24 hours'"),
        ("last_7d",  "NOW() - INTERVAL '7 days'"),
        ("last_30d", "NOW() - INTERVAL '30 days'"),
    ];

    let mut totals = serde_json::Map::new();
    for (label, interval) in windows {
        let sql = format!(
            "SELECT COUNT(*) FROM notification_costs WHERE sent_at >= {interval}"
        );
        let count: i64 = sqlx::query_scalar(&sql)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);
        totals.insert(label.to_string(), serde_json::json!(count));
    }

    // Per-channel delivery success rate (based on webhook_failures vs notification_costs)
    let channel_rows = sqlx::query(
        r#"
        SELECT
            nc.id::text               AS channel_id,
            nc.name                   AS channel_name,
            nc.channel_type           AS channel_type,
            COUNT(DISTINCT ncost.id)  AS sent,
            COUNT(DISTINCT wf.id)     AS failed
        FROM notification_channels nc
        LEFT JOIN notification_costs ncost ON ncost.channel_id = nc.id
            AND ncost.sent_at >= NOW() - INTERVAL '30 days'
        LEFT JOIN webhook_failures wf ON wf.url = (nc.config->>'url')
            AND wf.created_at >= NOW() - INTERVAL '30 days'
        GROUP BY nc.id, nc.name, nc.channel_type
        "#
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let channels: Vec<serde_json::Value> = channel_rows
        .into_iter()
        .map(|r| {
            let sent: i64 = r.try_get("sent").unwrap_or(0);
            let failed: i64 = r.try_get("failed").unwrap_or(0);
            let success_rate = if sent == 0 {
                1.0_f64
            } else {
                (sent - failed).max(0) as f64 / sent as f64
            };
            serde_json::json!({
                "channel_id":   r.try_get::<String, _>("channel_id").unwrap_or_default(),
                "channel_name": r.try_get::<String, _>("channel_name").unwrap_or_default(),
                "channel_type": r.try_get::<String, _>("channel_type").unwrap_or_default(),
                "sent_30d":     sent,
                "failed_30d":   failed,
                "success_rate": (success_rate * 1000.0).round() / 1000.0,
            })
        })
        .collect();

    // Top-10 most-notified contracts (from events table, last 30d)
    let top_contracts = sqlx::query(
        r#"
        SELECT contract_id, COUNT(*) AS event_count
        FROM events
        WHERE created_at >= NOW() - INTERVAL '30 days'
        GROUP BY contract_id
        ORDER BY event_count DESC
        LIMIT 10
        "#
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|r| {
        serde_json::json!({
            "contract_id":  r.try_get::<String, _>("contract_id").unwrap_or_default(),
            "event_count":  r.try_get::<i64, _>("event_count").unwrap_or(0),
        })
    })
    .collect::<Vec<_>>();

    Json(serde_json::json!({
        "totals":       totals,
        "channels":     channels,
        "top_contracts": top_contracts,
    }))
}

/// GET /v1/admin/notifications/costs
///
/// Returns cumulative notification cost breakdown by channel and optional time period.
pub async fn notification_costs(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let period = params.get("period").map(|s| s.as_str()).unwrap_or("30d");
    let interval = match period {
        "24h" => "24 hours",
        "7d"  => "7 days",
        "30d" => "30 days",
        _     => "30 days",
    };

    let sql = format!(
        r#"
        SELECT
            channel_id::text,
            channel_name,
            channel_type,
            COUNT(*)          AS notification_count,
            SUM(cost_cents)   AS total_cost_cents
        FROM notification_costs
        WHERE sent_at >= NOW() - INTERVAL '{interval}'
        GROUP BY channel_id, channel_name, channel_type
        ORDER BY total_cost_cents DESC
        "#
    );

    let rows = sqlx::query(&sql)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();

    let breakdown: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            let total_cents: i64 = r.try_get("total_cost_cents").unwrap_or(0);
            serde_json::json!({
                "channel_id":          r.try_get::<String, _>("channel_id").unwrap_or_default(),
                "channel_name":        r.try_get::<String, _>("channel_name").unwrap_or_default(),
                "channel_type":        r.try_get::<String, _>("channel_type").unwrap_or_default(),
                "notification_count":  r.try_get::<i64, _>("notification_count").unwrap_or(0),
                "total_cost_usd":      total_cents as f64 / 100.0,
            })
        })
        .collect();

    // Warn if monthly budget exceeded (config field, defaults to 0 = no limit)
    let monthly_budget_usd: f64 = state
        .config
        .notification_monthly_budget_usd
        .unwrap_or(0.0);

    let total_usd: f64 = breakdown
        .iter()
        .filter_map(|b| b["total_cost_usd"].as_f64())
        .sum();

    if monthly_budget_usd > 0.0 && total_usd >= monthly_budget_usd * 0.9 {
        tracing::warn!(
            total_usd = total_usd,
            budget_usd = monthly_budget_usd,
            "Notification cost is approaching or has exceeded the monthly budget"
        );
    }

    Json(serde_json::json!({
        "period":        period,
        "total_cost_usd": (total_usd * 100.0).round() / 100.0,
        "breakdown":     breakdown,
    }))
}

/// POST /v1/admin/notifications/channels/:id/test
///
/// Sends a test notification on the specified channel and returns the result.
pub async fn test_notification_channel(
    State(state): State<AppState>,
    Path(channel_id): Path<String>,
) -> impl IntoResponse {
    let channel_uuid = match channel_id.parse::<uuid::Uuid>() {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid channel id" })),
            ).into_response();
        }
    };

    let row = sqlx::query(
        "SELECT id::text AS channel_id, name, channel_type, config \
         FROM notification_channels WHERE id = $1"
    )
    .bind(channel_uuid)
    .fetch_optional(&state.pool)
    .await;

    let channel = match row {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "channel not found" })),
            ).into_response();
        }
        Err(e) => {
            tracing::error!(error = %e, "DB error looking up notification channel");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            ).into_response();
        }
    };

    let channel_name: String = channel.try_get("name").unwrap_or_default();
    let channel_type: String = channel.try_get("channel_type").unwrap_or_default();
    let config: serde_json::Value = channel.try_get("config").unwrap_or(serde_json::json!({}));
    // Issue #1162: sensitive config values are stored encrypted.
    let config = match crate::integration_secrets::open_config(&config) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "Failed to decrypt notification channel credentials");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "failed to decrypt channel credentials" })),
            ).into_response();
        }
    };

    let test_subject = format!("[TEST] Soroban Pulse notification test – channel '{channel_name}'");
    let test_body = format!(
        "[TEST] This is a test notification from Soroban Pulse.\nChannel: {channel_name}\nType: {channel_type}\nSent at: {}",
        chrono::Utc::now().to_rfc3339(),
    );

    let success = match channel_type.as_str() {
        "webhook" => {
            let url = config.get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if url.is_empty() {
                false
            } else {
                let client = reqwest::Client::new();
                let payload = serde_json::json!({
                    "subject": test_subject,
                    "body":    test_body,
                    "test":    true,
                });
                client.post(&url)
                    .header("Content-Type", "application/json")
                    .body(payload.to_string())
                    .send()
                    .await
                    .map(|r| r.status().is_success())
                    .unwrap_or(false)
            }
        }
        "email" | "sms" => {
            tracing::info!(
                channel_id = %channel_id,
                channel_type = %channel_type,
                subject = %test_subject,
                "Test notification logged (email/SMS delivery requires live credentials)"
            );
            true
        }
        _ => false,
    };

    crate::metrics::record_notification_test(&channel_type, success);

    let status = if success { StatusCode::OK } else { StatusCode::BAD_GATEWAY };
    (
        status,
        Json(serde_json::json!({
            "channel_id":   channel_id,
            "channel_name": channel_name,
            "channel_type": channel_type,
            "success":      success,
            "subject":      test_subject,
        })),
    ).into_response()
}

/// Request body for creating a notification channel (Issue #477).
#[derive(Debug, serde::Deserialize)]
pub struct CreateNotificationChannelRequest {
    pub name: String,
    pub channel_type: String,
    /// Channel-specific configuration (URL, recipients, …).
    #[serde(default)]
    pub config: Value,
    /// Optional content filter: only events whose data satisfies the predicate
    /// are delivered through this channel.
    #[serde(default)]
    pub content_filter: Option<crate::content_filter::ContentFilter>,
}

/// POST /v1/admin/notifications/channels — create a notification channel,
/// optionally with a content filter (Issue #477).
///
/// Invalid content filter expressions are rejected with `400 Bad Request`.
pub async fn create_notification_channel(
    State(state): State<AppState>,
    Json(req): Json<CreateNotificationChannelRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    if req.name.trim().is_empty() {
        return Err(AppError::Validation("name is required".to_string()));
    }
    if !matches!(
        req.channel_type.as_str(),
        "webhook" | "email" | "sms" | "slack" | "discord" | "telegram" | "pagerduty" | "github"
    ) {
        return Err(AppError::Validation(
            "channel_type must be one of: webhook, email, sms, slack, discord, telegram, pagerduty, github"
                .to_string(),
        ));
    }

    // Validate the content filter up front so invalid expressions return 400.
    let content_filter_json = match req.content_filter {
        Some(ref cf) => {
            cf.validate().map_err(AppError::Validation)?;
            Some(serde_json::to_value(cf).map_err(|e| AppError::Validation(e.to_string()))?)
        }
        None => None,
    };

    // Issue #1162: provider credentials (e.g. SMS auth tokens) in the channel
    // config are encrypted before they reach the database.
    let sealed_config = crate::integration_secrets::seal_config(&req.config)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO notification_channels (name, channel_type, config, content_filter) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(&req.name)
    .bind(&req.channel_type)
    .bind(&sealed_config)
    .bind(&content_filter_json)
    .fetch_one(&state.pool)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": id,
            "name": req.name,
            "channel_type": req.channel_type,
            "content_filter": content_filter_json,
        })),
    ))
}

// ---------------------------------------------------------------------------
// Issue #487 – Email open tracking
// ---------------------------------------------------------------------------

/// 1×1 transparent GIF used as the tracking pixel.
const TRACKING_PIXEL_GIF: &[u8] = &[
    0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00,
    0x00, 0xff, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00,
    0x01, 0x00, 0x00, 0x02, 0x00, 0x3b,
];

/// Record an email open event and return the 1×1 tracking pixel.
/// GET /v1/notifications/email/track/:token
pub async fn track_email_open(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> impl IntoResponse {
    // Verify the HMAC-signed, time-limited token (issue #1158).
    // The subject is the DB token string; verification failure silently
    // returns the pixel anyway (to avoid leaking validity to enumerators)
    // but does NOT record an open event.
    let db_subject = match crate::email_token::verify_token(
        &token,
        Some(&crate::email_token::TokenPurpose::Open),
        None,
    ) {
        Ok(claims) => Some(claims.subject),
        Err(_) => None,
    };

    if let Some(subject) = db_subject {
        let pool = state.pool.clone();
        tokio::spawn(async move {
            let updated = sqlx::query_scalar::<_, i64>(
                "WITH upd AS (
                    UPDATE email_opens SET opened_at = NOW()
                    WHERE token = $1 AND opened_at IS NULL
                    RETURNING 1
                 ) SELECT COUNT(*) FROM upd",
            )
            .bind(&subject)
            .fetch_one(&pool)
            .await
            .unwrap_or(0);
            if updated > 0 {
                crate::metrics::record_email_open();
            }
        });
    }

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/gif")
        .header(header::CACHE_CONTROL, "no-cache, no-store, must-revalidate")
        .header("pragma", "no-cache")
        .body(Body::from(TRACKING_PIXEL_GIF.to_vec()))
        .unwrap()
}

/// Return email open-rate statistics.
/// GET /v1/admin/notifications/email/stats
pub async fn get_email_stats(
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM email_opens",
    )
    .fetch_one(&state.read_pool)
    .await?;

    let opened: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM email_opens WHERE opened_at IS NOT NULL",
    )
    .fetch_one(&state.read_pool)
    .await?;

    let open_rate = if total == 0 {
        0.0_f64
    } else {
        opened as f64 / total as f64 * 100.0
    };

    Ok(Json(json!({
        "total_sent": total,
        "total_opened": opened,
        "open_rate_pct": open_rate,
    })))
}

// ---------------------------------------------------------------------------
// Issue #488 – Email click tracking
// ---------------------------------------------------------------------------

/// Record an email link click and redirect to the destination URL.
/// GET /v1/notifications/email/click/:token
pub async fn track_email_click(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> impl IntoResponse {
    // Verify the HMAC-signed, time-limited token (issue #1158).
    let db_subject = match crate::email_token::verify_token(
        &token,
        Some(&crate::email_token::TokenPurpose::Click),
        None,
    ) {
        Ok(claims) => claims.subject,
        Err(_) => {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("click token not found"))
                .unwrap();
        }
    };

    let dest: Option<String> = sqlx::query_scalar(
        "SELECT destination_url FROM email_clicks WHERE token = $1",
    )
    .bind(&db_subject)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let pool = state.pool.clone();
    let subject_clone = db_subject.clone();
    tokio::spawn(async move {
        let updated = sqlx::query_scalar::<_, i64>(
            "WITH upd AS (
                UPDATE email_clicks SET clicked_at = NOW()
                WHERE token = $1 AND clicked_at IS NULL
                RETURNING 1
             ) SELECT COUNT(*) FROM upd",
        )
        .bind(&subject_clone)
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
        if updated > 0 {
            crate::metrics::record_email_click();
        }
    });

    match dest {
        Some(url) => Response::builder()
            .status(StatusCode::FOUND)
            .header(header::LOCATION, url.as_str())
            .body(Body::empty())
            .unwrap(),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("click token not found"))
            .unwrap(),
    }
}

// ---------------------------------------------------------------------------
// Issue #489 – A/B test results
// ---------------------------------------------------------------------------

/// Return A/B test delivery and open-rate statistics.
/// GET /v1/admin/notifications/email/ab-test/results
pub async fn get_ab_test_results(
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let rows = sqlx::query(
        "SELECT d.ab_template,
                COUNT(d.id)                               AS deliveries,
                COUNT(o.id) FILTER (WHERE o.opened_at IS NOT NULL) AS opens
         FROM email_deliveries d
         LEFT JOIN email_opens o
               ON o.email_notification_id = d.email_notification_id
              AND o.recipient = d.recipient
         WHERE d.ab_template IS NOT NULL
         GROUP BY d.ab_template
         ORDER BY d.ab_template",
    )
    .fetch_all(&state.read_pool)
    .await?;

    let results: Vec<Value> = rows
        .iter()
        .map(|row| {
            let template: Option<String> = row.try_get("ab_template").ok();
            let deliveries: i64 = row.try_get("deliveries").unwrap_or(0);
            let opens: i64 = row.try_get("opens").unwrap_or(0);
            let open_rate = if deliveries == 0 {
                0.0_f64
            } else {
                opens as f64 / deliveries as f64 * 100.0
            };
            json!({
                "template": template,
                "deliveries": deliveries,
                "opens": opens,
                "open_rate_pct": open_rate,
            })
        })
        .collect();

    Ok(Json(json!({ "results": results })))
}

// ---------------------------------------------------------------------------
// Issue #490 – Suppression list management
// ---------------------------------------------------------------------------

/// Add an email address or webhook URL to the suppression list.
/// POST /v1/admin/notifications/suppress
pub async fn add_suppression(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let target = body
        .get("target")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("missing 'target' field".to_string()))?
        .to_string();

    let target_type = body
        .get("target_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("missing 'target_type' field".to_string()))?
        .to_string();

    if target_type != "email" && target_type != "webhook" {
        return Err(AppError::Validation(
            "target_type must be 'email' or 'webhook'".to_string(),
        ));
    }

    let reason = body.get("reason").and_then(|v| v.as_str()).map(|s| s.to_string());
    let expires_at: Option<chrono::DateTime<chrono::Utc>> = body
        .get("expires_at")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok());

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO suppression_lists (target, target_type, reason, expires_at) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (target, target_type) DO UPDATE \
             SET reason = EXCLUDED.reason, expires_at = EXCLUDED.expires_at \
         RETURNING id",
    )
    .bind(&target)
    .bind(&target_type)
    .bind(&reason)
    .bind(expires_at)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(json!({
        "id": id,
        "target": target,
        "target_type": target_type,
        "status": "suppressed",
    })))
}

/// Remove an entry from the suppression list.
/// DELETE /v1/admin/notifications/suppress/:id
pub async fn remove_suppression(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let deleted: Option<String> = sqlx::query_scalar(
        "DELETE FROM suppression_lists WHERE id = $1 RETURNING target",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;

    match deleted {
        Some(target) => Ok(Json(json!({ "id": id, "target": target, "status": "removed" }))),
        None => Err(AppError::NotFound),
    }
}
