/// Request body for creating a notification channel.
#[derive(Debug, Deserialize)]
pub struct CreateChannelRequest {
    pub name: String,
    pub channel_type: String,
    pub config: Value,
    pub retry_policy: Option<Value>,
}

/// Request body for updating a notification channel (all fields optional.
#[derive(Debug, Deserialize, Default)]
pub struct UpdateChannelRequest {
    pub name: Option<String>,
    pub config: Option<Value>,
    pub retry_policy: Option<Value>,
}

/// Request body for cloning a channel (optional overrides applied to the copy).
#[derive(Debug, Deserialize, Default)]
pub struct CloneChannelRequest {
    pub name: Option<String>,
    pub config: Option<Value>,
    pub retry_policy: Option<Value>,
}

/// Single channel entry inside an import payload.
#[derive(Debug, Deserialize)]
pub struct ImportChannelEntry {
    pub name: String,
    pub channel_type: String,
    pub config: Value,
    pub retry_policy: Option<Value>,
}

/// Request body for POST /v1/admin/notifications/channels/import.
#[derive(Debug, Deserialize)]
pub struct ImportChannelsRequest {
    pub channels: Vec<ImportChannelEntry>,
}


#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub page: i64,
    pub limit: i64,
    pub total: i64,
    pub has_more: bool,
}

// ── #511: Notification channel bulk operations ────────────────────────────────

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct BulkChannelRequest {
    pub channel_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct BulkTagRequest {
    pub channel_ids: Vec<Uuid>,
    pub tags: Vec<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct BulkChannelResult {
    pub id: Uuid,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct BulkOperationResponse {
    pub succeeded: i64,
    pub failed: i64,
    pub results: Vec<BulkChannelResult>,
}

// ── #512: Notification system lifecycle webhooks ──────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SystemWebhookConfig {
    pub id: Uuid,
    pub url: String,
    pub secret: Option<String>,
    /// Lifecycle event types this webhook subscribes to.
    /// Supported: channel_created, channel_deleted, channel_failed,
    ///            delivery_failed, queue_backed_up, or "*" for all.
    pub events: Vec<String>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateSystemWebhookRequest {
    pub url: String,
    pub secret: Option<String>,
    pub events: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct LifecycleEvent {
    pub event_type: String,
    pub channel_id: Option<Uuid>,
    pub channel_name: Option<String>,
    pub message: String,
    pub occurred_at: DateTime<Utc>,
    pub metadata: Value,
}

// ── #513: Notification delivery SLA monitoring ────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NotificationDelivery {
    pub id: Uuid,
    pub channel_id: Uuid,
    pub channel_name: String,
    pub event_indexed_at: DateTime<Utc>,
    pub delivered_at: DateTime<Utc>,
    pub latency_seconds: f64,
    pub sla_breached: bool,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RecordDeliveryRequest {
    pub channel_id: Uuid,
    pub channel_name: String,
    pub event_indexed_at: DateTime<Utc>,
    pub delivered_at: DateTime<Utc>,
}

// ── #514: Notification capacity planning ─────────────────────────────────────

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct NotificationCapacityResponse {
    pub current_rate_per_minute: f64,
    pub projected_rate_per_minute: f64,
    pub growth_trend_percent: f64,
    pub channels: Vec<ChannelCapacityInfo>,
    pub computed_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ChannelCapacityInfo {
    pub channel_id: Uuid,
    pub channel_name: String,
    pub current_rate_per_minute: f64,
    pub estimated_time_to_limit_minutes: Option<f64>,
    pub recommendations: Vec<String>,
}

impl<T> PaginatedResponse<T> {
    pub fn new(data: Vec<T>, page: i64, limit: i64, total: i64) -> Self {
        let has_more = (page * limit) < total;
        Self {
            data,
            page,
            limit,
            total,
            has_more,
        }
    }
}

// ── Notification Channel models (#507 #508 #509 #510) ───────────────────────

/// A managed notification channel (webhook, email, or SMS).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct NotificationChannel {
    pub id: Uuid,
    pub name: String,
    pub channel_type: String,
    pub config: Value,
    pub retry_policy: Value,
    #[sqlx(default)]
    pub description: Option<String>,
    #[sqlx(default)]
    pub tags: Vec<String>,
    /// SHA-256 hex of the creator's API key (#508).
    #[sqlx(default)]
    pub owner: Option<String>,
    #[sqlx(default)]
    pub status: String,
    #[sqlx(default)]
    pub contract_filter: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateNotificationChannelRequest {
    pub name: String,
    pub channel_type: String,
    pub config: Value,
    #[serde(default)]
    pub retry_policy: Option<Value>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub contract_filter: Vec<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateNotificationChannelRequest {
    pub name: Option<String>,
    pub config: Option<Value>,
    pub retry_policy: Option<Value>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub status: Option<String>,
    pub contract_filter: Option<Vec<String>>,
}

/// Query parameters for listing/searching notification channels (#509 #510).
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct NotificationChannelSearchParams {
    /// Full-text search on name and description.
    pub q: Option<String>,
    pub channel_type: Option<String>,
    /// Filter by contract_id present in channel's contract_filter list.
    pub contract_id: Option<String>,
    pub status: Option<String>,
    /// Filter by tag (#509).
    pub tag: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

impl NotificationChannelSearchParams {
    pub fn effective_page(&self) -> i64 {
        self.page.unwrap_or(1).max(1)
    }
    pub fn effective_limit(&self) -> i64 {
        self.page_size.unwrap_or(20).clamp(1, 100)
    }
    pub fn offset(&self) -> i64 {
        (self.effective_page() - 1) * self.effective_limit()
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct AddTagRequest {
    pub tag: String,
}

// ── Channel Group models (#507) ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct NotificationChannelGroup {
    pub id: Uuid,
    pub name: String,
    #[sqlx(default)]
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateChannelGroupRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Channel IDs to include in this group.
    #[serde(default)]
    pub channel_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ChannelGroupResponse {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub channel_ids: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}


// ── Issue #627: Contract existence check ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
