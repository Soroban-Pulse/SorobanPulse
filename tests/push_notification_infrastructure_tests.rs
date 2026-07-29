//! Integration tests for push notification infrastructure (Issue #839).
//!
//! Tests cover:
//! - Web Push (VAPID) configuration and auth header building
//! - Exponential backoff retry logic
//! - Delivery analytics tracking
//! - Notification preference management (quiet hours, validation)
//! - Web Push subscription serialization

use soroban_pulse::push_notification::{
    build_vapid_auth_header, DeliveryAnalytics, DeviceType, NotificationPreferences,
    PushWorkerConfig, RetryConfig, VapidConfig, WebPushKeys, WebPushSubscription,
};

// ---------------------------------------------------------------------------
// VAPID / Web Push
// ---------------------------------------------------------------------------

#[test]
fn vapid_config_from_env_returns_none_when_unset() {
    std::env::remove_var("VAPID_PUBLIC_KEY");
    std::env::remove_var("VAPID_PRIVATE_KEY");
    std::env::remove_var("VAPID_SUBJECT");
    assert!(VapidConfig::from_env().is_none());
}

#[test]
fn vapid_auth_header_format() {
    let config = VapidConfig {
        public_key: "BNcRdTEST".to_string(),
        private_key: "privkey".to_string(),
        subject: "mailto:test@sorobanpulse.io".to_string(),
    };
    let header =
        build_vapid_auth_header(&config, "https://fcm.googleapis.com/push/v1/token123").unwrap();
    assert!(header.starts_with("vapid t="));
    assert!(header.contains(",k=BNcRdTEST"));
}

#[test]
fn vapid_auth_header_rejects_bad_url() {
    let config = VapidConfig {
        public_key: "k".to_string(),
        private_key: "p".to_string(),
        subject: "mailto:a@b.com".to_string(),
    };
    assert!(build_vapid_auth_header(&config, "not-a-url").is_err());
}

#[test]
fn web_push_subscription_json_roundtrip() {
    let sub = WebPushSubscription {
        endpoint: "https://push.services.mozilla.com/wpush/v2/abc123".to_string(),
        keys: WebPushKeys {
            p256dh: "BNcRdTEST".to_string(),
            auth: "tBHITEST".to_string(),
        },
    };
    let json_str = serde_json::to_string(&sub).unwrap();
    let parsed: WebPushSubscription = serde_json::from_str(&json_str).unwrap();
    assert_eq!(parsed.endpoint, sub.endpoint);
    assert_eq!(parsed.keys.p256dh, sub.keys.p256dh);
    assert_eq!(parsed.keys.auth, sub.keys.auth);
}

// ---------------------------------------------------------------------------
// Worker config
// ---------------------------------------------------------------------------

#[test]
fn push_worker_enabled_with_vapid() {
    let cfg = PushWorkerConfig {
        fcm: None,
        apns: None,
        vapid: Some(VapidConfig {
            public_key: "pub".to_string(),
            private_key: "priv".to_string(),
            subject: "mailto:a@b.com".to_string(),
        }),
        retry: RetryConfig::from_env(),
    };
    assert!(cfg.is_enabled());
}

#[test]
fn push_worker_disabled_with_no_config() {
    let cfg = PushWorkerConfig {
        fcm: None,
        apns: None,
        vapid: None,
        retry: RetryConfig {
            max_retries: 5,
            base_delay_secs: 2,
            max_delay_secs: 3600,
        },
    };
    assert!(!cfg.is_enabled());
}

// ---------------------------------------------------------------------------
// Retry logic
// ---------------------------------------------------------------------------

#[test]
fn retry_exponential_backoff_increases() {
    let cfg = RetryConfig {
        max_retries: 5,
        base_delay_secs: 2,
        max_delay_secs: 3600,
    };
    let d0 = cfg.next_delay(0).unwrap().as_secs();
    let d1 = cfg.next_delay(1).unwrap().as_secs();
    let d2 = cfg.next_delay(2).unwrap().as_secs();
    assert!(d1 >= d0, "delay should increase");
    assert!(d2 >= d1, "delay should increase");
}

#[test]
fn retry_returns_none_after_max() {
    let cfg = RetryConfig {
        max_retries: 3,
        base_delay_secs: 1,
        max_delay_secs: 100,
    };
    assert!(cfg.next_delay(0).is_some());
    assert!(cfg.next_delay(2).is_some());
    assert!(cfg.next_delay(3).is_none());
    assert!(cfg.next_delay(100).is_none());
}

#[test]
fn retry_delay_capped() {
    let cfg = RetryConfig {
        max_retries: 20,
        base_delay_secs: 100,
        max_delay_secs: 500,
    };
    let d = cfg.next_delay(10).unwrap().as_secs();
    // base * 2^10 = 102400, capped at 500 + jitter
    assert!(d <= 600, "delay {d} should be capped near max_delay_secs");
}

#[test]
fn retry_is_exhausted_check() {
    let cfg = RetryConfig {
        max_retries: 3,
        base_delay_secs: 1,
        max_delay_secs: 100,
    };
    assert!(!cfg.is_exhausted(0));
    assert!(!cfg.is_exhausted(2));
    assert!(cfg.is_exhausted(3));
}

// ---------------------------------------------------------------------------
// Delivery analytics
// ---------------------------------------------------------------------------

#[test]
fn analytics_initial_state_all_zeros() {
    let a = DeliveryAnalytics::new();
    let snap = a.snapshot();
    assert_eq!(snap["total_sent"], 0);
    assert_eq!(snap["total_failed"], 0);
    assert_eq!(snap["total_retries"], 0);
    assert_eq!(snap["by_platform"]["android"], 0);
    assert_eq!(snap["by_platform"]["ios"], 0);
    assert_eq!(snap["by_platform"]["web"], 0);
}

#[test]
fn analytics_tracks_per_platform() {
    let a = DeliveryAnalytics::new();
    a.record_sent(DeviceType::Android);
    a.record_sent(DeviceType::Ios);
    a.record_sent(DeviceType::Ios);
    a.record_sent(DeviceType::Web);

    let snap = a.snapshot();
    assert_eq!(snap["total_sent"], 4);
    assert_eq!(snap["by_platform"]["android"], 1);
    assert_eq!(snap["by_platform"]["ios"], 2);
    assert_eq!(snap["by_platform"]["web"], 1);
}

#[test]
fn analytics_delivery_rate_calculation() {
    let a = DeliveryAnalytics::new();
    for _ in 0..9 {
        a.record_sent(DeviceType::Android);
    }
    a.record_failed();

    let snap = a.snapshot();
    let rate = snap["delivery_rate_percent"].as_f64().unwrap();
    assert!((rate - 90.0).abs() < 0.1);
}

#[test]
fn analytics_zero_division_returns_zero() {
    let a = DeliveryAnalytics::new();
    let snap = a.snapshot();
    assert_eq!(snap["delivery_rate_percent"], 0.0);
}

#[test]
fn analytics_tracks_retries_and_invalid() {
    let a = DeliveryAnalytics::new();
    a.record_retry();
    a.record_retry();
    a.record_retry();
    a.record_invalid_token();

    let snap = a.snapshot();
    assert_eq!(snap["total_retries"], 3);
    assert_eq!(snap["total_invalid_tokens"], 1);
}

// ---------------------------------------------------------------------------
// Notification preferences
// ---------------------------------------------------------------------------

#[test]
fn preferences_default_all_active() {
    let prefs = NotificationPreferences::default();
    assert!(prefs.enabled);
    for h in 0..24 {
        assert!(prefs.is_within_active_hours(h));
    }
}

#[test]
fn preferences_quiet_hours_non_wrapping() {
    let prefs = NotificationPreferences {
        quiet_hours_start: Some(2),
        quiet_hours_end: Some(6),
        ..Default::default()
    };
    assert!(prefs.is_within_active_hours(0));
    assert!(prefs.is_within_active_hours(1));
    assert!(!prefs.is_within_active_hours(2));
    assert!(!prefs.is_within_active_hours(5));
    assert!(prefs.is_within_active_hours(6));
    assert!(prefs.is_within_active_hours(12));
}

#[test]
fn preferences_quiet_hours_wrapping_midnight() {
    let prefs = NotificationPreferences {
        quiet_hours_start: Some(22),
        quiet_hours_end: Some(6),
        ..Default::default()
    };
    assert!(!prefs.is_within_active_hours(22));
    assert!(!prefs.is_within_active_hours(23));
    assert!(!prefs.is_within_active_hours(0));
    assert!(!prefs.is_within_active_hours(5));
    assert!(prefs.is_within_active_hours(6));
    assert!(prefs.is_within_active_hours(21));
}

#[test]
fn preferences_validate_rejects_invalid_hours() {
    let bad_start = NotificationPreferences {
        quiet_hours_start: Some(25),
        ..Default::default()
    };
    assert!(bad_start.validate().is_err());

    let bad_end = NotificationPreferences {
        quiet_hours_end: Some(30),
        ..Default::default()
    };
    assert!(bad_end.validate().is_err());
}

#[test]
fn preferences_validate_accepts_valid() {
    let prefs = NotificationPreferences {
        quiet_hours_start: Some(22),
        quiet_hours_end: Some(6),
        ..Default::default()
    };
    assert!(prefs.validate().is_ok());
}

#[test]
fn preferences_serialization_roundtrip() {
    let prefs = NotificationPreferences {
        enabled: true,
        quiet_hours_start: Some(22),
        quiet_hours_end: Some(6),
        min_interval_secs: Some(300),
        event_types: vec!["transfer".to_string(), "mint".to_string()],
    };
    let json = serde_json::to_string(&prefs).unwrap();
    let parsed: NotificationPreferences = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.quiet_hours_start, Some(22));
    assert_eq!(parsed.quiet_hours_end, Some(6));
    assert_eq!(parsed.min_interval_secs, Some(300));
    assert_eq!(parsed.event_types.len(), 2);
}

// ---------------------------------------------------------------------------
// DeviceType
// ---------------------------------------------------------------------------

#[test]
fn device_type_all_variants_roundtrip() {
    for (input, expected) in [
        ("android", DeviceType::Android),
        ("ios", DeviceType::Ios),
        ("web", DeviceType::Web),
    ] {
        let dt = DeviceType::from_str(input).unwrap();
        assert_eq!(dt, expected);
        assert_eq!(dt.as_str(), input);
    }
}

#[test]
fn device_type_case_insensitive() {
    assert_eq!(DeviceType::from_str("ANDROID"), Some(DeviceType::Android));
    assert_eq!(DeviceType::from_str("IOS"), Some(DeviceType::Ios));
    assert_eq!(DeviceType::from_str("WEB"), Some(DeviceType::Web));
    assert_eq!(DeviceType::from_str("Web"), Some(DeviceType::Web));
}

#[test]
fn device_type_unknown_returns_none() {
    assert!(DeviceType::from_str("blackberry").is_none());
    assert!(DeviceType::from_str("").is_none());
    assert!(DeviceType::from_str("windows_phone").is_none());
}
