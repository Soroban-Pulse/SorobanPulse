-- Issue #1162: Encrypt integration credentials at rest.
--
-- Credential columns now hold an AES-256-GCM envelope produced by
-- src/encryption.rs (`secrets` module), stored as text with an `enc:v1:` prefix
-- and keyed by INTEGRATION_ENCRYPTION_KEY (separate from the event_data key).
--
-- Columns holding a credential or secret-bearing URL:
--   github_integrations.access_token      GitHub token
--   discord_integrations.webhook_url      Discord webhook URL (embeds the webhook token)
--   slack_integrations.webhook_url        Slack incoming webhook URL (secret)
--   slack_integrations.bot_token          Slack bot token
--   slack_integrations.signing_secret     Slack signing secret
--   telegram_integrations.bot_token       Telegram bot token
--   pagerduty_integrations.routing_key    PagerDuty Events API routing key
--   pagerduty_integrations.api_key        PagerDuty REST API key
--   notification_channels.config          sensitive keys (auth_token, api_key, ...) in JSONB
--
-- Existing rows cannot be encrypted in SQL because the key lives only in the
-- application. The backfill runs at application startup
-- (integration_secrets::backfill_plaintext) and via POST /v1/admin/reencrypt.
--
-- The CHECK constraints below are created NOT VALID: PostgreSQL enforces them
-- for every new or updated row immediately, without failing on legacy rows
-- that have not been backfilled yet. Once the backfill has run, validate them:
--
--   ALTER TABLE github_integrations    VALIDATE CONSTRAINT github_integrations_access_token_encrypted;
--   ALTER TABLE discord_integrations   VALIDATE CONSTRAINT discord_integrations_webhook_url_encrypted;
--   ALTER TABLE slack_integrations     VALIDATE CONSTRAINT slack_integrations_webhook_url_encrypted;
--   ALTER TABLE slack_integrations     VALIDATE CONSTRAINT slack_integrations_bot_token_encrypted;
--   ALTER TABLE slack_integrations     VALIDATE CONSTRAINT slack_integrations_signing_secret_encrypted;
--   ALTER TABLE telegram_integrations  VALIDATE CONSTRAINT telegram_integrations_bot_token_encrypted;
--   ALTER TABLE pagerduty_integrations VALIDATE CONSTRAINT pagerduty_integrations_routing_key_encrypted;
--   ALTER TABLE pagerduty_integrations VALIDATE CONSTRAINT pagerduty_integrations_api_key_encrypted;

ALTER TABLE github_integrations
    ADD CONSTRAINT github_integrations_access_token_encrypted
    CHECK (access_token LIKE 'enc:v1:%') NOT VALID;

ALTER TABLE discord_integrations
    ADD CONSTRAINT discord_integrations_webhook_url_encrypted
    CHECK (webhook_url LIKE 'enc:v1:%') NOT VALID;

ALTER TABLE slack_integrations
    ADD CONSTRAINT slack_integrations_webhook_url_encrypted
    CHECK (webhook_url IS NULL OR webhook_url LIKE 'enc:v1:%') NOT VALID;

ALTER TABLE slack_integrations
    ADD CONSTRAINT slack_integrations_bot_token_encrypted
    CHECK (bot_token IS NULL OR bot_token LIKE 'enc:v1:%') NOT VALID;

ALTER TABLE slack_integrations
    ADD CONSTRAINT slack_integrations_signing_secret_encrypted
    CHECK (signing_secret IS NULL OR signing_secret LIKE 'enc:v1:%') NOT VALID;

ALTER TABLE telegram_integrations
    ADD CONSTRAINT telegram_integrations_bot_token_encrypted
    CHECK (bot_token LIKE 'enc:v1:%') NOT VALID;

ALTER TABLE pagerduty_integrations
    ADD CONSTRAINT pagerduty_integrations_routing_key_encrypted
    CHECK (routing_key LIKE 'enc:v1:%') NOT VALID;

ALTER TABLE pagerduty_integrations
    ADD CONSTRAINT pagerduty_integrations_api_key_encrypted
    CHECK (api_key IS NULL OR api_key LIKE 'enc:v1:%') NOT VALID;

COMMENT ON COLUMN github_integrations.access_token    IS 'Encrypted (enc:v1: AES-256-GCM envelope, INTEGRATION_ENCRYPTION_KEY)';
COMMENT ON COLUMN discord_integrations.webhook_url    IS 'Encrypted (enc:v1: AES-256-GCM envelope, INTEGRATION_ENCRYPTION_KEY)';
COMMENT ON COLUMN slack_integrations.webhook_url      IS 'Encrypted (enc:v1: AES-256-GCM envelope, INTEGRATION_ENCRYPTION_KEY)';
COMMENT ON COLUMN slack_integrations.bot_token        IS 'Encrypted (enc:v1: AES-256-GCM envelope, INTEGRATION_ENCRYPTION_KEY)';
COMMENT ON COLUMN slack_integrations.signing_secret   IS 'Encrypted (enc:v1: AES-256-GCM envelope, INTEGRATION_ENCRYPTION_KEY)';
COMMENT ON COLUMN telegram_integrations.bot_token     IS 'Encrypted (enc:v1: AES-256-GCM envelope, INTEGRATION_ENCRYPTION_KEY)';
COMMENT ON COLUMN pagerduty_integrations.routing_key  IS 'Encrypted (enc:v1: AES-256-GCM envelope, INTEGRATION_ENCRYPTION_KEY)';
COMMENT ON COLUMN pagerduty_integrations.api_key      IS 'Encrypted (enc:v1: AES-256-GCM envelope, INTEGRATION_ENCRYPTION_KEY)';
COMMENT ON COLUMN notification_channels.config        IS 'Sensitive keys (auth_token, api_key, bot_token, ...) hold enc:v1: envelopes';
