//! Event tagging system — Issue #936
//!
//! Flexible, hierarchical tags that can be applied to indexed events, both
//! manually (via the tag management endpoints) and automatically (via
//! property-matching rules). See `docs/event-tagging.md`.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct EventTag {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub parent_id: Option<Uuid>,
    pub color: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateTagRequest {
    pub name: String,
    pub description: Option<String>,
    pub parent_id: Option<Uuid>,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateAutoTagRuleRequest {
    pub tag_id: Uuid,
    pub event_type_match: Option<String>,
    pub property_path: Option<String>,
    pub property_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AutoTagRule {
    pub id: Uuid,
    pub tag_id: Uuid,
    pub event_type_match: Option<String>,
    pub property_path: Option<String>,
    pub property_value: Option<String>,
    pub active: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TagUsageMetric {
    pub tag_id: Uuid,
    pub tag_name: String,
    pub event_count: i64,
    pub manual_count: i64,
    pub auto_count: i64,
}

/// Creates a tag. `name` must be unique (Issue #936: "Create tags table
/// with relationships").
pub async fn create_tag(pool: &PgPool, req: &CreateTagRequest) -> Result<EventTag, sqlx::Error> {
    sqlx::query_as::<_, EventTag>(
        "INSERT INTO event_tags (name, description, parent_id, color) \
         VALUES ($1, $2, $3, $4) \
         RETURNING id, name, description, parent_id, color, created_at",
    )
    .bind(&req.name)
    .bind(&req.description)
    .bind(req.parent_id)
    .bind(&req.color)
    .fetch_one(pool)
    .await
}

pub async fn delete_tag(pool: &PgPool, tag_id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM event_tags WHERE id = $1")
        .bind(tag_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn list_tags(pool: &PgPool) -> Result<Vec<EventTag>, sqlx::Error> {
    sqlx::query_as::<_, EventTag>(
        "SELECT id, name, description, parent_id, color, created_at \
         FROM event_tags ORDER BY name",
    )
    .fetch_all(pool)
    .await
}

/// Returns the full ancestor chain of `tag_id`, root first, ending with
/// `tag_id` itself — the tag's position in the hierarchy/taxonomy (Issue
/// #936: "Implement tag hierarchy/taxonomy").
pub async fn tag_path(pool: &PgPool, tag_id: Uuid) -> Result<Vec<EventTag>, sqlx::Error> {
    let mut path = Vec::new();
    let mut current = Some(tag_id);
    // Bounded to avoid an infinite loop if a cycle is ever introduced
    // directly in the database.
    for _ in 0..64 {
        let Some(id) = current else { break };
        let tag = sqlx::query_as::<_, EventTag>(
            "SELECT id, name, description, parent_id, color, created_at \
             FROM event_tags WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        match tag {
            Some(t) => {
                current = t.parent_id;
                path.push(t);
            }
            None => break,
        }
    }
    path.reverse();
    Ok(path)
}

/// Applies `tag_id` to `event_id` (Issue #936: "Add tag management
/// endpoints"). Idempotent — re-applying the same tag is a no-op.
pub async fn apply_tag(
    pool: &PgPool,
    event_id: Uuid,
    tag_id: Uuid,
    source: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO event_tag_assignments (event_id, tag_id, source) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (event_id, tag_id) DO NOTHING",
    )
    .bind(event_id)
    .bind(tag_id)
    .bind(source)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn remove_tag(pool: &PgPool, event_id: Uuid, tag_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM event_tag_assignments WHERE event_id = $1 AND tag_id = $2")
        .bind(event_id)
        .bind(tag_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn tags_for_event(pool: &PgPool, event_id: Uuid) -> Result<Vec<EventTag>, sqlx::Error> {
    sqlx::query_as::<_, EventTag>(
        "SELECT t.id, t.name, t.description, t.parent_id, t.color, t.created_at \
         FROM event_tags t \
         JOIN event_tag_assignments a ON a.tag_id = t.id \
         WHERE a.event_id = $1 \
         ORDER BY t.name",
    )
    .bind(event_id)
    .fetch_all(pool)
    .await
}

/// Returns event ids carrying `tag_name`, most recent first (Issue #936:
/// "Implement tag-based filtering in queries").
pub async fn event_ids_by_tag(
    pool: &PgPool,
    tag_name: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT e.id FROM events e \
         JOIN event_tag_assignments a ON a.event_id = e.id \
         JOIN event_tags t ON t.id = a.tag_id \
         WHERE t.name = $1 \
         ORDER BY e.timestamp DESC \
         LIMIT $2 OFFSET $3",
    )
    .bind(tag_name)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

pub async fn create_auto_tag_rule(
    pool: &PgPool,
    req: &CreateAutoTagRuleRequest,
) -> Result<AutoTagRule, sqlx::Error> {
    sqlx::query_as::<_, AutoTagRule>(
        "INSERT INTO event_auto_tag_rules (tag_id, event_type_match, property_path, property_value) \
         VALUES ($1, $2, $3, $4) \
         RETURNING id, tag_id, event_type_match, property_path, property_value, active",
    )
    .bind(req.tag_id)
    .bind(&req.event_type_match)
    .bind(&req.property_path)
    .bind(&req.property_value)
    .fetch_one(pool)
    .await
}

async fn active_rules(pool: &PgPool) -> Result<Vec<AutoTagRule>, sqlx::Error> {
    sqlx::query_as::<_, AutoTagRule>(
        "SELECT id, tag_id, event_type_match, property_path, property_value, active \
         FROM event_auto_tag_rules WHERE active = TRUE",
    )
    .fetch_all(pool)
    .await
}

fn json_pointer_matches(event_data: &serde_json::Value, path: &str, expected: &str) -> bool {
    let segments: Vec<&str> = path
        .trim_start_matches('.')
        .split('.')
        .filter(|s| !s.is_empty())
        .collect();
    let mut current = event_data;
    for segment in segments {
        match current.get(segment) {
            Some(next) => current = next,
            None => return false,
        }
    }
    current.as_str() == Some(expected)
        || current.to_string().trim_matches('"') == expected
}

fn rule_matches(rule: &AutoTagRule, event_type: &str, event_data: &serde_json::Value) -> bool {
    if rule.event_type_match.is_none() && rule.property_path.is_none() {
        // A rule with no predicates matches nothing (Issue #936: auto-tagging
        // must be based on actual event properties, not applied blindly).
        return false;
    }
    if let Some(ref expected_type) = rule.event_type_match {
        if expected_type != event_type {
            return false;
        }
    }
    if let (Some(ref path), Some(ref value)) = (&rule.property_path, &rule.property_value) {
        if !json_pointer_matches(event_data, path, value) {
            return false;
        }
    }
    true
}

/// Evaluates every active auto-tagging rule against one event and applies
/// any that match (Issue #936: "Create auto-tagging based on event
/// properties"). Returns the tag ids that were applied.
pub async fn auto_tag_event(
    pool: &PgPool,
    event_id: Uuid,
    event_type: &str,
    event_data: &serde_json::Value,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let rules = active_rules(pool).await?;
    let mut applied = Vec::new();
    for rule in rules {
        if rule_matches(&rule, event_type, event_data) {
            apply_tag(pool, event_id, rule.tag_id, "auto").await?;
            applied.push(rule.tag_id);
        }
    }
    Ok(applied)
}

/// Per-tag usage counts, split by manual vs. auto-applied (Issue #936:
/// "Add tag analytics and usage metrics").
pub async fn tag_usage_metrics(pool: &PgPool) -> Result<Vec<TagUsageMetric>, sqlx::Error> {
    sqlx::query_as::<_, TagUsageMetric>(
        "SELECT t.id AS tag_id, t.name AS tag_name, \
                COUNT(a.event_id) AS event_count, \
                COUNT(a.event_id) FILTER (WHERE a.source = 'manual') AS manual_count, \
                COUNT(a.event_id) FILTER (WHERE a.source = 'auto') AS auto_count \
         FROM event_tags t \
         LEFT JOIN event_tag_assignments a ON a.tag_id = t.id \
         GROUP BY t.id, t.name \
         ORDER BY event_count DESC",
    )
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rule(event_type_match: Option<&str>, path: Option<&str>, value: Option<&str>) -> AutoTagRule {
        AutoTagRule {
            id: Uuid::nil(),
            tag_id: Uuid::nil(),
            event_type_match: event_type_match.map(String::from),
            property_path: path.map(String::from),
            property_value: value.map(String::from),
            active: true,
        }
    }

    #[test]
    fn rule_with_no_predicates_never_matches() {
        let r = rule(None, None, None);
        assert!(!rule_matches(&r, "contract", &json!({})));
    }

    #[test]
    fn rule_matches_on_event_type() {
        let r = rule(Some("contract"), None, None);
        assert!(rule_matches(&r, "contract", &json!({})));
        assert!(!rule_matches(&r, "system", &json!({})));
    }

    #[test]
    fn rule_matches_on_nested_property() {
        let r = rule(None, Some("action.kind"), Some("swap"));
        let data = json!({"action": {"kind": "swap"}});
        assert!(rule_matches(&r, "contract", &data));
        let other = json!({"action": {"kind": "transfer"}});
        assert!(!rule_matches(&r, "contract", &other));
    }

    #[test]
    fn rule_requires_both_type_and_property_when_both_set() {
        let r = rule(Some("contract"), Some("action.kind"), Some("swap"));
        let data = json!({"action": {"kind": "swap"}});
        assert!(rule_matches(&r, "contract", &data));
        assert!(!rule_matches(&r, "system", &data));
    }

    #[test]
    fn json_pointer_missing_path_does_not_match() {
        let data = json!({"a": {"b": "x"}});
        assert!(!json_pointer_matches(&data, "a.c", "x"));
    }
}
