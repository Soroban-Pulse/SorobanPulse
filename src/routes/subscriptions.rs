//! Subscription and notification routes: subscription lifecycle, delivery
//! settings, third-party integrations, and email tracking.

use axum::{routing::get, Router};

use super::AppState;
use crate::{handlers, subscriptions};

/// Public `/v1` subscription routes.
pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/subscriptions", axum::routing::post(subscriptions::create_subscription))
        .route("/subscriptions/{id}", get(subscriptions::get_subscription).delete(subscriptions::cancel_subscription))
        .route("/subscriptions/{id}/ack", axum::routing::post(subscriptions::ack_subscription))
        // Issue #619: Email notification config for subscriptions
        .route("/subscriptions/{id}/email", get(subscriptions::get_subscription_email).put(subscriptions::update_subscription_email))
        // Issue #620: Push notification config for subscriptions
        .route("/subscriptions/{id}/push", get(crate::push_notification::get_subscription_push).put(crate::push_notification::update_subscription_push))
        // Issue #839: Push notification preferences
        .route("/subscriptions/{id}/push/preferences", get(crate::push_notification::get_notification_preferences).put(crate::push_notification::update_notification_preferences))
        // Issue #628: Batch subscription config and delivery
        .route("/subscriptions/{id}/batch", get(subscriptions::get_subscription_batch_config).put(subscriptions::update_subscription_batch_config).post(subscriptions::deliver_batch))
        // Issue #884: Subscription pause/resume
        .route("/subscriptions/{id}/pause", axum::routing::post(subscriptions::pause_subscription))
        .route("/subscriptions/{id}/resume", axum::routing::post(subscriptions::resume_subscription))
        .route("/subscriptions/{id}/pause-status", get(subscriptions::get_pause_resume_status))
        // Issue #674: GitHub integration
        .route("/subscriptions/{id}/integrations/github", axum::routing::post(crate::integration_handlers::setup_github_integration).get(crate::integration_handlers::get_github_integration).delete(crate::integration_handlers::delete_github_integration))
        // Issue #675: Discord integration
        .route("/subscriptions/{id}/integrations/discord", axum::routing::post(crate::integration_handlers::setup_discord_integration).get(crate::integration_handlers::get_discord_integration).delete(crate::integration_handlers::delete_discord_integration))
        // Issue #676: Slack integration
        .route("/subscriptions/{id}/integrations/slack", axum::routing::post(crate::integration_handlers::setup_slack_integration).get(crate::integration_handlers::get_slack_integration).delete(crate::integration_handlers::delete_slack_integration))
        // Issue #677: Telegram integration
        .route("/subscriptions/{id}/integrations/telegram", axum::routing::post(crate::integration_handlers::setup_telegram_integration).get(crate::integration_handlers::get_telegram_integration).delete(crate::integration_handlers::delete_telegram_integration))
        // Issue #951: PagerDuty integration
        .route("/subscriptions/{id}/integrations/pagerduty", axum::routing::post(crate::integration_handlers::setup_pagerduty_integration).get(crate::integration_handlers::get_pagerduty_integration).delete(crate::integration_handlers::delete_pagerduty_integration))
        .route("/subscriptions/{id}/integrations/pagerduty/incidents", axum::routing::get(crate::integration_handlers::list_pagerduty_incidents))
        .route("/subscriptions/{id}/integrations/pagerduty/incidents/acknowledge", axum::routing::post(crate::integration_handlers::acknowledge_pagerduty_incident))
        .route("/subscriptions/{id}/integrations/pagerduty/incidents/resolve", axum::routing::post(crate::integration_handlers::resolve_pagerduty_incident))
        // Issue #487: email open tracking (public – email clients fetch the pixel)
        .route("/notifications/email/track/{token}", get(handlers::track_email_open))
        // Issue #488: email click tracking (public – email link redirect)
        .route("/notifications/email/click/{token}", get(handlers::track_email_click))
}

/// Admin-only subscription and notification routes; `super` applies the
/// admin auth layer.
pub(super) fn admin_router() -> Router<AppState> {
    Router::new()
        // Issue #882: Anomaly detection alerting
        .route("/admin/subscriptions/{subscription_id}/anomaly-config", axum::routing::post(handlers::create_anomaly_config))
        .route("/admin/subscriptions/{subscription_id}/anomaly-alerts", get(handlers::get_anomaly_alerts))
        .route("/admin/subscriptions/{subscription_id}/anomaly-alerts/{alert_id}/acknowledge", axum::routing::post(handlers::acknowledge_anomaly_alert))
        .route("/admin/notifications/channels", axum::routing::post(handlers::create_notification_channel))
        // Issue #487: email open stats
        .route("/admin/notifications/email/stats", get(handlers::get_email_stats))
        // Issue #489: A/B test results
        .route("/admin/notifications/email/ab-test/results", get(handlers::get_ab_test_results))
        // Issue #490: suppression list management
        .route("/admin/notifications/suppress", axum::routing::post(handlers::add_suppression))
        .route("/admin/notifications/suppress/{id}", axum::routing::delete(handlers::remove_suppression))
        // #839: Push notification delivery analytics
        .route("/admin/push/analytics", axum::routing::get(crate::push_notification::get_push_analytics))
}
