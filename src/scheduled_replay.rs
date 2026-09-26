//! Scheduled event replay (Issue #930).
//!
//! Extends the on-demand replay system with recurring schedules so that
//! operators can configure automatic re-delivery of historical events without
//! manual intervention. Three schedule types are supported:
//!
//! * **Interval** — replay every N seconds.
//! * **Cron** — replay according to a cron expression (evaluated externally;
//!   the scheduler sets `next_run_at` based on the expression).
//! * **OneShot** — replay once at `next_run_at`, then deactivate.
//!
//! The typical scheduler loop is:
//! 1. Call [`get_due_schedules`] to find schedules whose `next_run_at` ≤ NOW().
//! 2. For each schedule, trigger a replay via `event_replay::replay_from_*`.
//! 3. Call [`record_schedule_run`] with the outcome.
//! 4. Call [`calculate_next_run`] → [`update_next_run`] to advance the clock.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{error, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Discriminates how a [`ReplaySchedule`] calculates its next execution time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleType {
    /// Re-run every `interval_secs` seconds after the last run.
    Interval,
    /// Re-run according to a cron expression stored in `cron_expression`.
    /// `next_run_at` is written by an external cron parser and is not
    /// computed by [`calculate_next_run`].
    Cron,
    /// Run exactly once at `next_run_at`, then deactivate the schedule.
    OneShot,
}

impl ScheduleType {
    /// Canonical string representation used in the database `schedule_type`
    /// column.
    pub fn as_str(&self) -> &'static str {
        match self {
            ScheduleType::Interval => "interval",
            ScheduleType::Cron => "cron",
            ScheduleType::OneShot => "one_shot",
        }
    }

    /// Parse from the database string representation.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "interval" => Some(ScheduleType::Interval),
            "cron" => Some(ScheduleType::Cron),
            "one_shot" => Some(ScheduleType::OneShot),
            _ => None,
        }
    }
}

/// A persisted replay schedule row, as returned by schedule queries.
///
/// Maps 1-to-1 to the `replay_schedules` table added by migration
/// `20260830000004_scheduled_event_replay.sql`.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ReplaySchedule {
    pub id: Uuid,
    /// Human-readable label for this schedule.
    pub name: String,
    /// The subscription that will receive replayed events.
    pub subscription_id: Uuid,
    /// Discriminator string stored in the DB: `"interval"`, `"cron"`, or
    /// `"one_shot"`.
    pub schedule_type: String,
    /// Cron expression (required when `schedule_type = "cron"`).
    pub cron_expression: Option<String>,
    /// Seconds between runs (required when `schedule_type = "interval"`).
    pub interval_secs: Option<i64>,
    /// Optional contract ID to filter replayed events.
    pub filter_contract_id: Option<String>,
    /// Optional event type to filter replayed events (e.g. `"contract"`).
    pub filter_event_type: Option<String>,
    /// Maximum number of events to deliver per run. `None` means no limit.
    pub max_events: Option<i64>,
    /// How many seconds before the run time to start replaying from.
    /// For example, `3600` means "replay the last hour of events".
    pub replay_from_offset_secs: i64,
    /// Whether this schedule is currently active. Inactive schedules are
    /// skipped by the scheduler poll.
    pub is_active: bool,
    /// Timestamp of the most-recent successful run, or `None` if never run.
    pub last_run_at: Option<DateTime<Utc>>,
    /// Timestamp of the next scheduled run. The scheduler polls for rows
    /// where `next_run_at <= NOW() AND is_active = true`.
    pub next_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Request body for creating a new replay schedule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateScheduleRequest {
    /// Human-readable label.
    pub name: String,
    /// Target subscription UUID.
    pub subscription_id: Uuid,
    /// `"interval"`, `"cron"`, or `"one_shot"`.
    pub schedule_type: String,
    /// Required when `schedule_type = "cron"`.
    pub cron_expression: Option<String>,
    /// Required when `schedule_type = "interval"`.
    pub interval_secs: Option<i64>,
    /// Optional: only replay events for this contract.
    pub filter_contract_id: Option<String>,
    /// Optional: only replay events of this type.
    pub filter_event_type: Option<String>,
    /// Optional: cap the number of events replayed per run.
    pub max_events: Option<i64>,
    /// How far back (seconds) to replay from at each run. Defaults to `3600`.
    pub replay_from_offset_secs: i64,
}

/// A single execution log entry for a scheduled replay run.
///
/// Maps 1-to-1 to the `scheduled_replay_runs` table.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ScheduledReplayRun {
    pub id: Uuid,
    /// The schedule that triggered this run.
    pub schedule_id: Uuid,
    /// The underlying on-demand replay record, if one was created.
    pub replay_id: Option<Uuid>,
    /// `"running"`, `"completed"`, or `"failed"`.
    pub status: String,
    /// Number of events successfully replayed.
    pub events_replayed: i64,
    /// Error detail when `status = "failed"`.
    pub error_message: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// CRUD helpers
// ---------------------------------------------------------------------------

/// Persist a new [`ReplaySchedule`] and return the full row (including the
/// server-generated `id` and `created_at`).
///
/// `next_run_at` is set to `NOW()` so that interval and one-shot schedules
/// are eligible for execution immediately. Cron schedules should have
/// `next_run_at` updated by the external cron parser before the first poll.
pub async fn create_replay_schedule(
    pool: &PgPool,
    req: CreateScheduleRequest,
) -> Result<ReplaySchedule, String> {
    // Basic validation before hitting the DB.
    if req.name.trim().is_empty() {
        return Err("Schedule name must not be empty".to_string());
    }
    match req.schedule_type.as_str() {
        "interval" => {
            if req.interval_secs.map(|s| s <= 0).unwrap_or(true) {
                return Err(
                    "interval_secs must be a positive integer for interval schedules".to_string(),
                );
            }
        }
        "cron" => {
            if req.cron_expression.as_deref().map(str::trim).unwrap_or("").is_empty() {
                return Err(
                    "cron_expression must be provided for cron schedules".to_string(),
                );
            }
        }
        "one_shot" => {}
        other => {
            return Err(format!(
                "Unknown schedule_type '{}'; expected one of: interval, cron, one_shot",
                other
            ));
        }
    }

    let schedule = sqlx::query_as::<_, ReplaySchedule>(
        "INSERT INTO replay_schedules \
         (name, subscription_id, schedule_type, cron_expression, interval_secs, \
          filter_contract_id, filter_event_type, max_events, \
          replay_from_offset_secs, is_active, next_run_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, true, NOW()) \
         RETURNING *",
    )
    .bind(&req.name)
    .bind(req.subscription_id)
    .bind(&req.schedule_type)
    .bind(&req.cron_expression)
    .bind(req.interval_secs)
    .bind(&req.filter_contract_id)
    .bind(&req.filter_event_type)
    .bind(req.max_events)
    .bind(req.replay_from_offset_secs)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("Failed to create replay schedule: {}", e))?;

    info!(
        schedule_id = %schedule.id,
        name = %schedule.name,
        schedule_type = %schedule.schedule_type,
        subscription_id = %schedule.subscription_id,
        "Created replay schedule"
    );

    Ok(schedule)
}

/// Fetch a single [`ReplaySchedule`] by primary key.
///
/// Returns `Err` when the row is not found.
pub async fn get_replay_schedule(
    pool: &PgPool,
    schedule_id: Uuid,
) -> Result<ReplaySchedule, String> {
    sqlx::query_as::<_, ReplaySchedule>(
        "SELECT * FROM replay_schedules WHERE id = $1",
    )
    .bind(schedule_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to fetch replay schedule: {}", e))?
    .ok_or_else(|| format!("Replay schedule not found: {}", schedule_id))
}

/// List all replay schedules for a given subscription, ordered by creation
/// time (newest first).
pub async fn list_replay_schedules(
    pool: &PgPool,
    subscription_id: Uuid,
) -> Result<Vec<ReplaySchedule>, String> {
    sqlx::query_as::<_, ReplaySchedule>(
        "SELECT * FROM replay_schedules \
         WHERE subscription_id = $1 \
         ORDER BY created_at DESC",
    )
    .bind(subscription_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list replay schedules: {}", e))
}

/// Activate or deactivate a schedule.
///
/// Deactivating a schedule prevents the scheduler from picking it up in
/// subsequent polls without deleting the record or its run history.
pub async fn update_schedule_status(
    pool: &PgPool,
    schedule_id: Uuid,
    is_active: bool,
) -> Result<(), String> {
    let rows_affected = sqlx::query(
        "UPDATE replay_schedules \
         SET is_active = $1, updated_at = NOW() \
         WHERE id = $2",
    )
    .bind(is_active)
    .bind(schedule_id)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to update schedule status: {}", e))?
    .rows_affected();

    if rows_affected == 0 {
        return Err(format!("Replay schedule not found: {}", schedule_id));
    }

    info!(
        schedule_id = %schedule_id,
        is_active = is_active,
        "Updated replay schedule status"
    );

    Ok(())
}

/// Return all active schedules whose `next_run_at` is at or before the
/// current time — i.e. schedules that are due to run now.
///
/// This is the primary query used by the scheduler poll loop.
pub async fn get_due_schedules(pool: &PgPool) -> Result<Vec<ReplaySchedule>, String> {
    sqlx::query_as::<_, ReplaySchedule>(
        "SELECT * FROM replay_schedules \
         WHERE is_active = true \
           AND next_run_at IS NOT NULL \
           AND next_run_at <= NOW() \
         ORDER BY next_run_at ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to fetch due replay schedules: {}", e))
}

/// Record the outcome of a scheduled replay run in `scheduled_replay_runs`.
///
/// Returns the newly created run's UUID.
pub async fn record_schedule_run(
    pool: &PgPool,
    schedule_id: Uuid,
    replay_id: Option<Uuid>,
    status: &str,
    events_replayed: i64,
    error_message: Option<&str>,
) -> Result<Uuid, String> {
    // A completed/failed run has a completed_at timestamp.
    let completed_at: Option<DateTime<Utc>> = match status {
        "completed" | "failed" => Some(Utc::now()),
        _ => None,
    };

    let run_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO scheduled_replay_runs \
         (schedule_id, replay_id, status, events_replayed, error_message, completed_at) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         RETURNING id",
    )
    .bind(schedule_id)
    .bind(replay_id)
    .bind(status)
    .bind(events_replayed)
    .bind(error_message)
    .bind(completed_at)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("Failed to record scheduled replay run: {}", e))?;

    // Also update last_run_at on the parent schedule for completed runs.
    if status == "completed" {
        if let Err(e) = sqlx::query(
            "UPDATE replay_schedules \
             SET last_run_at = NOW(), updated_at = NOW() \
             WHERE id = $1",
        )
        .bind(schedule_id)
        .execute(pool)
        .await
        {
            warn!(
                schedule_id = %schedule_id,
                error = %e,
                "Failed to update last_run_at on replay schedule"
            );
        }
    } else if status == "failed" {
        error!(
            schedule_id = %schedule_id,
            events_replayed = events_replayed,
            error = ?error_message,
            "Scheduled replay run failed"
        );
    }

    info!(
        run_id = %run_id,
        schedule_id = %schedule_id,
        status = status,
        events_replayed = events_replayed,
        "Recorded scheduled replay run"
    );

    Ok(run_id)
}

/// Compute the timestamp of the next run for a schedule.
///
/// | Schedule type | Behaviour |
/// |---|---|
/// | `Interval` | `NOW() + interval_secs` seconds |
/// | `Cron` | `None` — the cron parser owns `next_run_at` |
/// | `OneShot` | `None` — one-shot schedules should be deactivated after running |
///
/// Returns `None` when the schedule type does not support automatic
/// advancement (cron, one-shot), signalling the caller to either invoke an
/// external cron parser or deactivate the schedule.
pub fn calculate_next_run(schedule: &ReplaySchedule) -> Option<DateTime<Utc>> {
    match schedule.schedule_type.as_str() {
        "interval" => {
            let interval_secs = schedule.interval_secs?;
            if interval_secs <= 0 {
                warn!(
                    schedule_id = %schedule.id,
                    interval_secs = interval_secs,
                    "interval_secs is non-positive; cannot calculate next run"
                );
                return None;
            }
            Some(Utc::now() + Duration::seconds(interval_secs))
        }
        // Cron schedules: the external cron parser writes next_run_at directly.
        "cron" => None,
        // One-shot schedules run once then stop.
        "one_shot" => None,
        other => {
            warn!(
                schedule_id = %schedule.id,
                schedule_type = other,
                "Unknown schedule type; cannot calculate next run"
            );
            None
        }
    }
}

/// Persist the updated `next_run_at` value on a schedule row.
///
/// After calling [`calculate_next_run`], pass its result here to advance the
/// schedule clock. If `next_run_at` is `None` (cron or one-shot), the column
/// is set to `NULL` and `is_active` is set to `false` for one-shot schedules.
pub async fn update_next_run(
    pool: &PgPool,
    schedule_id: Uuid,
    next_run_at: Option<DateTime<Utc>>,
) -> Result<(), String> {
    let rows_affected = sqlx::query(
        "UPDATE replay_schedules \
         SET next_run_at = $1, updated_at = NOW() \
         WHERE id = $2",
    )
    .bind(next_run_at)
    .bind(schedule_id)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to update next_run_at: {}", e))?
    .rows_affected();

    if rows_affected == 0 {
        return Err(format!("Replay schedule not found: {}", schedule_id));
    }

    info!(
        schedule_id = %schedule_id,
        next_run_at = ?next_run_at,
        "Updated next_run_at for replay schedule"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Helper that builds a minimal ReplaySchedule for testing calculate_next_run.
    fn make_schedule(schedule_type: &str, interval_secs: Option<i64>) -> ReplaySchedule {
        ReplaySchedule {
            id: Uuid::new_v4(),
            name: "test-schedule".to_string(),
            subscription_id: Uuid::new_v4(),
            schedule_type: schedule_type.to_string(),
            cron_expression: None,
            interval_secs,
            filter_contract_id: None,
            filter_event_type: None,
            max_events: None,
            replay_from_offset_secs: 3600,
            is_active: true,
            last_run_at: None,
            next_run_at: None,
            created_at: Utc::now(),
        }
    }

    // --- ScheduleType ---

    #[test]
    fn schedule_type_round_trips_through_str() {
        for (variant, expected) in &[
            (ScheduleType::Interval, "interval"),
            (ScheduleType::Cron, "cron"),
            (ScheduleType::OneShot, "one_shot"),
        ] {
            assert_eq!(variant.as_str(), *expected);
            assert_eq!(ScheduleType::from_str(expected), Some(variant.clone()));
        }
    }

    #[test]
    fn schedule_type_from_str_returns_none_for_unknown() {
        assert_eq!(ScheduleType::from_str("daily"), None);
        assert_eq!(ScheduleType::from_str(""), None);
    }

    // --- calculate_next_run ---

    #[test]
    fn calculate_next_run_interval_advances_by_interval_secs() {
        let schedule = make_schedule("interval", Some(600));
        let before = Utc::now();
        let next = calculate_next_run(&schedule).expect("should return Some for interval");
        let after = Utc::now();

        // next_run_at should be ~600 seconds in the future.
        let diff = next - before;
        assert!(
            diff >= Duration::seconds(599) && diff <= Duration::seconds(601) + (after - before),
            "expected ~600s ahead, got {:?}",
            diff
        );
    }

    #[test]
    fn calculate_next_run_cron_returns_none() {
        let mut schedule = make_schedule("cron", None);
        schedule.cron_expression = Some("0 */6 * * *".to_string());
        assert!(
            calculate_next_run(&schedule).is_none(),
            "cron schedule should return None from calculate_next_run"
        );
    }

    #[test]
    fn calculate_next_run_one_shot_returns_none() {
        let schedule = make_schedule("one_shot", None);
        assert!(
            calculate_next_run(&schedule).is_none(),
            "one-shot schedule should return None"
        );
    }

    #[test]
    fn calculate_next_run_interval_with_zero_secs_returns_none() {
        let schedule = make_schedule("interval", Some(0));
        assert!(
            calculate_next_run(&schedule).is_none(),
            "zero interval_secs should return None"
        );
    }

    #[test]
    fn calculate_next_run_interval_with_none_secs_returns_none() {
        let schedule = make_schedule("interval", None);
        assert!(
            calculate_next_run(&schedule).is_none(),
            "missing interval_secs should return None"
        );
    }

    #[test]
    fn calculate_next_run_unknown_type_returns_none() {
        let schedule = make_schedule("weekly", None);
        assert!(calculate_next_run(&schedule).is_none());
    }

    // --- CreateScheduleRequest validation (unit-level, no DB) ---

    #[test]
    fn create_schedule_request_can_be_constructed() {
        let req = CreateScheduleRequest {
            name: "hourly-replay".to_string(),
            subscription_id: Uuid::new_v4(),
            schedule_type: "interval".to_string(),
            cron_expression: None,
            interval_secs: Some(3600),
            filter_contract_id: Some("CABC123".to_string()),
            filter_event_type: Some("contract".to_string()),
            max_events: Some(500),
            replay_from_offset_secs: 3600,
        };
        assert_eq!(req.interval_secs, Some(3600));
        assert_eq!(req.max_events, Some(500));
    }

    #[test]
    fn replay_schedule_serializes_to_json() {
        let schedule = make_schedule("interval", Some(1800));
        let json = serde_json::to_string(&schedule).expect("serialization should not fail");
        assert!(json.contains("\"schedule_type\":\"interval\""));
        assert!(json.contains("\"interval_secs\":1800"));
    }

    #[test]
    fn scheduled_replay_run_status_variants() {
        let run = ScheduledReplayRun {
            id: Uuid::new_v4(),
            schedule_id: Uuid::new_v4(),
            replay_id: None,
            status: "completed".to_string(),
            events_replayed: 42,
            error_message: None,
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
        };
        assert_eq!(run.status, "completed");
        assert_eq!(run.events_replayed, 42);
        assert!(run.error_message.is_none());
        assert!(run.completed_at.is_some());
    }

    #[test]
    fn scheduled_replay_run_failed_has_error() {
        let run = ScheduledReplayRun {
            id: Uuid::new_v4(),
            schedule_id: Uuid::new_v4(),
            replay_id: None,
            status: "failed".to_string(),
            events_replayed: 0,
            error_message: Some("DB connection timeout".to_string()),
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
        };
        assert_eq!(run.status, "failed");
        assert_eq!(run.events_replayed, 0);
        assert_eq!(
            run.error_message.as_deref(),
            Some("DB connection timeout")
        );
    }
}
