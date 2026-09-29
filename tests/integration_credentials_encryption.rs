//! Issue #1162: integration credentials must never appear in plaintext in the
//! database or in API responses.
//!
//! Each test creates integrations through the real handlers, then "dumps" the
//! integration tables (every row serialised with `row_to_json`) and asserts no
//! plaintext secret appears anywhere in the dump.

use axum::{extract::Path, extract::State, response::IntoResponse, Json};
use serde_json::{json, Value};
use soroban_pulse::integration_handlers::{
    self, DiscordIntegrationRequest, GitHubIntegrationRequest, PagerDutyIntegrationRequest,
    SlackIntegrationRequest, TelegramIntegrationRequest,
};
use soroban_pulse::integration_secrets;
use sqlx::PgPool;
use uuid::Uuid;

const KEY: [u8; 32] = [0x3c; 32];

const GITHUB_TOKEN: &str = "ghp_plaintextGithubToken0001";
const DISCORD_URL: &str = "https://discord.com/api/webhooks/123/plaintextDiscordSecret0002";
const SLACK_URL: &str = "https://hooks.slack.com/services/T0/B0/plaintextSlackSecret0003";
const SLACK_BOT_TOKEN: &str = "xoxb-plaintextSlackBotToken0004";
const TELEGRAM_TOKEN: &str = "123456:plaintextTelegramToken0005";
const PD_ROUTING_KEY: &str = "plaintextPagerDutyRoutingKey0006";
const PD_API_KEY: &str = "plaintextPagerDutyApiKey0007";

const ALL_SECRETS: &[&str] = &[
    GITHUB_TOKEN,
    DISCORD_URL,
    SLACK_URL,
    SLACK_BOT_TOKEN,
    TELEGRAM_TOKEN,
    PD_ROUTING_KEY,
    PD_API_KEY,
];

const TABLES: &[&str] = &[
    "github_integrations",
    "discord_integrations",
    "slack_integrations",
    "telegram_integrations",
    "pagerduty_integrations",
    "notification_channels",
];

async fn new_subscription(pool: &PgPool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO subscriptions (callback_url, from_ledger) VALUES ('https://example.com/cb', 0) RETURNING id",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Serialise every row of every credential-bearing table, like a logical dump.
async fn dump(pool: &PgPool) -> String {
    let mut out = String::new();
    for table in TABLES {
        let rows: Vec<String> =
            sqlx::query_scalar(&format!("SELECT row_to_json(t)::text FROM {table} t"))
                .fetch_all(pool)
                .await
                .unwrap();
        for row in rows {
            out.push_str(&row);
            out.push('\n');
        }
    }
    out
}

async fn body_json(resp: impl IntoResponse) -> Value {
    let resp = resp.into_response();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn create_all_integrations(pool: &PgPool, sub: Uuid) {
    integration_handlers::setup_github_integration(
        State(pool.clone()),
        Path(sub),
        Json(GitHubIntegrationRequest {
            access_token: GITHUB_TOKEN.into(),
            owner: "o".into(),
            repository: "r".into(),
            issue_title_template: None,
            issue_body_template: None,
            auto_create_issues: None,
            pr_comment_enabled: None,
        }),
    )
    .await
    .unwrap();

    integration_handlers::setup_discord_integration(
        State(pool.clone()),
        Path(sub),
        Json(DiscordIntegrationRequest {
            webhook_url: DISCORD_URL.into(),
            bot_name: None,
            avatar_url: None,
            embed_enabled: None,
            thread_support: None,
        }),
    )
    .await
    .unwrap();

    integration_handlers::setup_slack_integration(
        State(pool.clone()),
        Path(sub),
        Json(SlackIntegrationRequest {
            webhook_url: Some(SLACK_URL.into()),
            bot_token: Some(SLACK_BOT_TOKEN.into()),
            channel: "#alerts".into(),
            block_kit_enabled: None,
            thread_support: None,
            user_mentions_enabled: None,
        }),
    )
    .await
    .unwrap();

    integration_handlers::setup_telegram_integration(
        State(pool.clone()),
        Path(sub),
        Json(TelegramIntegrationRequest {
            bot_token: TELEGRAM_TOKEN.into(),
            chat_id: "42".into(),
            webhook_enabled: None,
            webhook_url: None,
            message_thread_support: None,
            button_support: None,
        }),
    )
    .await
    .unwrap();

    integration_handlers::setup_pagerduty_integration(
        State(pool.clone()),
        Path(sub),
        Json(PagerDutyIntegrationRequest {
            routing_key: PD_ROUTING_KEY.into(),
            service_name: None,
            api_key: Some(PD_API_KEY.into()),
            escalation_policy_id: None,
            contract_filter: None,
            event_type_filter: None,
            severity_mapping: None,
            auto_resolve: None,
            auto_resolve_threshold_min: None,
        }),
    )
    .await
    .unwrap();
}

#[sqlx::test(migrations = "./migrations")]
async fn db_dump_contains_no_plaintext_credentials(pool: PgPool) {
    integration_secrets::init_keys(Some(KEY), None);
    let sub = new_subscription(&pool).await;
    create_all_integrations(&pool, sub).await;

    let dump = dump(&pool).await;
    for secret in ALL_SECRETS {
        assert!(!dump.contains(secret), "plaintext credential found in DB dump: {secret}");
    }
    assert!(dump.contains("enc:v1:"), "expected encrypted envelopes in DB dump");
}

#[sqlx::test(migrations = "./migrations")]
async fn api_responses_return_masked_credentials(pool: PgPool) {
    integration_secrets::init_keys(Some(KEY), None);
    let sub = new_subscription(&pool).await;
    create_all_integrations(&pool, sub).await;

    let responses = vec![
        body_json(integration_handlers::get_github_integration(State(pool.clone()), Path(sub)).await.unwrap()).await,
        body_json(integration_handlers::get_discord_integration(State(pool.clone()), Path(sub)).await.unwrap()).await,
        body_json(integration_handlers::get_slack_integration(State(pool.clone()), Path(sub)).await.unwrap()).await,
        body_json(integration_handlers::get_telegram_integration(State(pool.clone()), Path(sub)).await.unwrap()).await,
        body_json(integration_handlers::get_pagerduty_integration(State(pool.clone()), Path(sub)).await.unwrap()).await,
    ];
    let all = Value::Array(responses.clone()).to_string();
    for secret in ALL_SECRETS {
        assert!(!all.contains(secret), "plaintext credential in API response: {secret}");
    }
    assert!(!all.contains("enc:v1:"), "ciphertext must not be returned either");

    assert_eq!(responses[0]["access_token"], json!("****0001"));
    assert_eq!(responses[1]["webhook_url"], json!("****0002"));
    assert_eq!(responses[2]["webhook_url"], json!("****0003"));
    assert_eq!(responses[2]["bot_token"], json!("****0004"));
    assert_eq!(responses[3]["bot_token"], json!("****0005"));
    assert_eq!(responses[4]["routing_key"], json!("****0006"));
    assert_eq!(responses[4]["api_key"], json!("****0007"));
}

#[sqlx::test(migrations = "./migrations")]
async fn backfill_encrypts_legacy_plaintext_rows(pool: PgPool) {
    integration_secrets::init_keys(Some(KEY), None);
    let sub = new_subscription(&pool).await;

    // Simulate rows written before Issue #1162 (bypass the NOT VALID check).
    sqlx::query("ALTER TABLE telegram_integrations DROP CONSTRAINT telegram_integrations_bot_token_encrypted")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO telegram_integrations (subscription_id, bot_token, chat_id) VALUES ($1, $2, '1')")
        .bind(sub)
        .bind(TELEGRAM_TOKEN)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO notification_channels (name, channel_type, config) VALUES ('sms', 'sms', $1)",
    )
    .bind(json!({"provider": "twilio", "auth_token": "plaintextSmsAuthToken0008"}))
    .execute(&pool)
    .await
    .unwrap();

    let before = dump(&pool).await;
    assert!(before.contains(TELEGRAM_TOKEN));
    assert!(before.contains("plaintextSmsAuthToken0008"));

    let n = integration_secrets::backfill_plaintext(&pool).await.unwrap();
    assert_eq!(n, 2);

    let after = dump(&pool).await;
    assert!(!after.contains(TELEGRAM_TOKEN));
    assert!(!after.contains("plaintextSmsAuthToken0008"));

    // Stored values still decrypt to the original credential.
    let stored: String = sqlx::query_scalar("SELECT bot_token FROM telegram_integrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(integration_secrets::open(&stored).unwrap(), TELEGRAM_TOKEN);

    // Idempotent.
    assert_eq!(integration_secrets::backfill_plaintext(&pool).await.unwrap(), 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn database_rejects_new_plaintext_credentials(pool: PgPool) {
    let sub = new_subscription(&pool).await;
    let result = sqlx::query(
        "INSERT INTO github_integrations (subscription_id, access_token, owner, repository) VALUES ($1, $2, 'o', 'r')",
    )
    .bind(sub)
    .bind(GITHUB_TOKEN)
    .execute(&pool)
    .await;
    assert!(result.is_err(), "CHECK constraint must reject plaintext tokens");
}
