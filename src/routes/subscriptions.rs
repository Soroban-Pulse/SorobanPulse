//! Subscription routes, including per-subscription integrations (served under `/v1`).

#[allow(unused_imports)]
use super::*;

/// Subscription lifecycle, notification config, integrations and anomaly alerting routes.
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/subscriptions", axum::routing::post(crate::subscriptions::create_subscription))
        .route("/subscriptions/{id}", get(crate::subscriptions::get_subscription).delete(crate::subscriptions::cancel_subscription))
        .route("/subscriptions/{id}/ack", axum::routing::post(crate::subscriptions::ack_subscription))
        // Issue #619: Email notification config for subscriptions
        .route("/subscriptions/{id}/email", get(crate::subscriptions::get_subscription_email).put(crate::subscriptions::update_subscription_email))
        // Issue #620: Push notification config for subscriptions
        .route("/subscriptions/{id}/push", get(crate::push_notification::get_subscription_push).put(crate::push_notification::update_subscription_push))
        // Issue #839: Push notification preferences
        .route("/subscriptions/{id}/push/preferences", get(crate::push_notification::get_notification_preferences).put(crate::push_notification::update_notification_preferences))
        // Issue #628: Batch subscription config and delivery
        .route("/subscriptions/{id}/batch", get(crate::subscriptions::get_subscription_batch_config).put(crate::subscriptions::update_subscription_batch_config).post(crate::subscriptions::deliver_batch))
        // Issue #884: Subscription pause/resume
        .route("/subscriptions/{id}/pause", axum::routing::post(crate::subscriptions::pause_subscription))
        .route("/subscriptions/{id}/resume", axum::routing::post(crate::subscriptions::resume_subscription))
        .route("/subscriptions/{id}/pause-status", get(crate::subscriptions::get_pause_resume_status))
        // Issue #882: Anomaly detection alerting
        .route("/admin/subscriptions/{subscription_id}/anomaly-config", axum::routing::post(handlers::create_anomaly_config))
        .route("/admin/subscriptions/{subscription_id}/anomaly-alerts", get(handlers::get_anomaly_alerts))
        .route("/admin/subscriptions/{subscription_id}/anomaly-alerts/{alert_id}/acknowledge", axum::routing::post(handlers::acknowledge_anomaly_alert))
        // Issue #674: GitHub integration
        .route("/subscriptions/{id}/integrations/github", axum::routing::post(crate::integration_handlers::setup_github_integration).get(crate::integration_handlers::get_github_integration).delete(crate::integration_handlers::delete_github_integration))
        // Issue #675: Discord integration
        .route("/subscriptions/{id}/integrations/discord", axum::routing::post(crate::integration_handlers::setup_discord_integration).get(crate::integration_handlers::get_discord_integration).delete(crate::integration_handlers::delete_discord_integration))
        // Issue #1348: Microsoft Teams integration
        .route("/subscriptions/{id}/integrations/teams", axum::routing::post(crate::integration_handlers::setup_teams_integration).get(crate::integration_handlers::get_teams_integration).delete(crate::integration_handlers::delete_teams_integration))
        .route("/subscriptions/{id}/test", axum::routing::post(crate::integration_handlers::test_subscription_integrations))
        // Issue #676: Slack integration
        .route("/subscriptions/{id}/integrations/slack", axum::routing::post(crate::integration_handlers::setup_slack_integration).get(crate::integration_handlers::get_slack_integration).delete(crate::integration_handlers::delete_slack_integration))
        // Issue #677: Telegram integration
        .route("/subscriptions/{id}/integrations/telegram", axum::routing::post(crate::integration_handlers::setup_telegram_integration).get(crate::integration_handlers::get_telegram_integration).delete(crate::integration_handlers::delete_telegram_integration))
        // Issue #951: PagerDuty integration
        .route("/subscriptions/{id}/integrations/pagerduty", axum::routing::post(crate::integration_handlers::setup_pagerduty_integration).get(crate::integration_handlers::get_pagerduty_integration).delete(crate::integration_handlers::delete_pagerduty_integration))
        .route("/subscriptions/{id}/integrations/pagerduty/incidents", axum::routing::get(crate::integration_handlers::list_pagerduty_incidents))
        .route("/subscriptions/{id}/integrations/pagerduty/incidents/acknowledge", axum::routing::post(crate::integration_handlers::acknowledge_pagerduty_incident))
        .route("/subscriptions/{id}/integrations/pagerduty/incidents/resolve", axum::routing::post(crate::integration_handlers::resolve_pagerduty_incident))
}
