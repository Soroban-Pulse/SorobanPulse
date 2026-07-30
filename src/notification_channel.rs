/// Unified Notification Channel Interface (Issue #809)
///
/// A minimal common contract that notification channels (Discord, Telegram,
/// Slack, etc.) can implement so callers can send an event notification
/// without knowing which concrete channel they're talking to. Existing
/// channel-specific methods (e.g. thread replies, buttons) remain available
/// directly on each client; this trait covers the shared "send an event"
/// path described in `docs/notification-channels.md`.
use crate::discord::DiscordClient;
use crate::models::SorobanEvent;
use crate::telegram::TelegramClient;

#[async_trait::async_trait]
pub trait NotificationChannel: Send + Sync {
    /// Short, stable identifier for this channel (e.g. "discord", "telegram").
    fn channel_name(&self) -> &'static str;

    /// Send a notification for `event` through this channel, returning a
    /// provider-specific message identifier on success.
    async fn send_event_notification(
        &self,
        event: &SorobanEvent,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>>;
}

#[async_trait::async_trait]
impl NotificationChannel for DiscordClient {
    fn channel_name(&self) -> &'static str {
        "discord"
    }

    async fn send_event_notification(
        &self,
        event: &SorobanEvent,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        DiscordClient::send_event_notification(self, event, None).await
    }
}

#[async_trait::async_trait]
impl NotificationChannel for TelegramClient {
    fn channel_name(&self) -> &'static str {
        "telegram"
    }

    async fn send_event_notification(
        &self,
        event: &SorobanEvent,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        TelegramClient::send_event_notification(self, event).await
    }
}
