//! Notification delivery routes: email tracking, bounces and suppression (served under `/v1`).

#[allow(unused_imports)]
use super::*;

/// Notification channel, email tracking/bounce and suppression routes.
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/notifications/channels", axum::routing::post(handlers::create_notification_channel))
        .route("/notifications/email/bounce", axum::routing::post(handlers::email_bounce_webhook))
        // Issue #487: email open tracking (public – email clients fetch the pixel)
        .route("/notifications/email/track/{token}", get(handlers::track_email_open))
        // Issue #487: email open stats (admin)
        .route("/admin/notifications/email/stats", get(handlers::get_email_stats))
        // Issue #488: email click tracking (public – email link redirect)
        .route("/notifications/email/click/{token}", get(handlers::track_email_click))
        // Issue #489: A/B test results (admin)
        .route("/admin/notifications/email/ab-test/results", get(handlers::get_ab_test_results))
        // Issue #490: suppression list management (admin)
        .route("/admin/notifications/suppress", axum::routing::post(handlers::add_suppression))
        .route("/admin/notifications/suppress/{id}", axum::routing::delete(handlers::remove_suppression))
}
