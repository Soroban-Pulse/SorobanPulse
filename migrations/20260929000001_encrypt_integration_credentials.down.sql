-- Reverts 20260929000001_encrypt_integration_credentials.sql.
-- Note: this only removes the constraints; values stay encrypted and still
-- require INTEGRATION_ENCRYPTION_KEY to be read.

ALTER TABLE github_integrations    DROP CONSTRAINT IF EXISTS github_integrations_access_token_encrypted;
ALTER TABLE discord_integrations   DROP CONSTRAINT IF EXISTS discord_integrations_webhook_url_encrypted;
ALTER TABLE slack_integrations     DROP CONSTRAINT IF EXISTS slack_integrations_webhook_url_encrypted;
ALTER TABLE slack_integrations     DROP CONSTRAINT IF EXISTS slack_integrations_bot_token_encrypted;
ALTER TABLE slack_integrations     DROP CONSTRAINT IF EXISTS slack_integrations_signing_secret_encrypted;
ALTER TABLE telegram_integrations  DROP CONSTRAINT IF EXISTS telegram_integrations_bot_token_encrypted;
ALTER TABLE pagerduty_integrations DROP CONSTRAINT IF EXISTS pagerduty_integrations_routing_key_encrypted;
ALTER TABLE pagerduty_integrations DROP CONSTRAINT IF EXISTS pagerduty_integrations_api_key_encrypted;

COMMENT ON COLUMN github_integrations.access_token    IS NULL;
COMMENT ON COLUMN discord_integrations.webhook_url    IS NULL;
COMMENT ON COLUMN slack_integrations.webhook_url      IS NULL;
COMMENT ON COLUMN slack_integrations.bot_token        IS NULL;
COMMENT ON COLUMN slack_integrations.signing_secret   IS NULL;
COMMENT ON COLUMN telegram_integrations.bot_token     IS NULL;
COMMENT ON COLUMN pagerduty_integrations.routing_key  IS NULL;
COMMENT ON COLUMN pagerduty_integrations.api_key      IS NULL;
COMMENT ON COLUMN notification_channels.config        IS NULL;
