use lettre::message::{header, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{interval, sleep};
use tracing::{error, info, warn};

use crate::{metrics, models::SorobanEvent, retry_policy::RetryPolicy};

/// A bounced recipient extracted from a provider webhook payload (Issue #484).
#[derive(Debug, Clone, PartialEq)]
pub struct BouncedRecipient {
    pub email: String,
    pub reason: Option<String>,
    pub provider: String,
}

/// Parse a bounce webhook payload from SendGrid, AWS SES (including SNS-wrapped
/// notifications), or Mailgun and return the bounced recipient addresses.
/// Best-effort: unknown shapes yield an empty vec rather than an error.
pub fn extract_bounced_recipients(payload: &serde_json::Value) -> Vec<BouncedRecipient> {
    let mut out = Vec::new();

    // SendGrid posts a JSON array of event objects.
    if let Some(events) = payload.as_array() {
        for ev in events {
            let event = ev.get("event").and_then(|v| v.as_str()).unwrap_or("");
            if matches!(event, "bounce" | "dropped" | "blocked") {
                if let Some(email) = ev.get("email").and_then(|v| v.as_str()) {
                    out.push(BouncedRecipient {
                        email: email.to_string(),
                        reason: ev.get("reason").and_then(|v| v.as_str()).map(String::from),
                        provider: "sendgrid".to_string(),
                    });
                }
            }
        }
        return out;
    }

    // AWS SES delivered via SNS: the notification is a JSON string under "Message".
    if payload.get("Type").and_then(|v| v.as_str()) == Some("Notification") {
        if let Some(msg) = payload.get("Message").and_then(|v| v.as_str()) {
            if let Ok(inner) = serde_json::from_str::<serde_json::Value>(msg) {
                return extract_bounced_recipients(&inner);
            }
        }
    }

    // AWS SES direct bounce notification.
    if payload.get("notificationType").and_then(|v| v.as_str()) == Some("Bounce") {
        if let Some(recipients) = payload
            .get("bounce")
            .and_then(|b| b.get("bouncedRecipients"))
            .and_then(|r| r.as_array())
        {
            for r in recipients {
                if let Some(email) = r.get("emailAddress").and_then(|v| v.as_str()) {
                    out.push(BouncedRecipient {
                        email: email.to_string(),
                        reason: r
                            .get("diagnosticCode")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                        provider: "ses".to_string(),
                    });
                }
            }
        }
        return out;
    }

    // Mailgun: modern "event-data" envelope or legacy flat object.
    let mailgun_event = payload
        .get("event-data")
        .and_then(|d| d.get("event"))
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("event").and_then(|v| v.as_str()));
    if let Some(event) = mailgun_event {
        if matches!(event, "failed" | "bounced" | "dropped" | "rejected") {
            let recipient = payload
                .get("event-data")
                .and_then(|d| d.get("recipient"))
                .and_then(|v| v.as_str())
                .or_else(|| payload.get("recipient").and_then(|v| v.as_str()));
            if let Some(email) = recipient {
                let reason = payload
                    .get("event-data")
                    .and_then(|d| d.get("reason"))
                    .and_then(|v| v.as_str())
                    .map(String::from);
                out.push(BouncedRecipient {
                    email: email.to_string(),
                    reason,
                    provider: "mailgun".to_string(),
                });
            }
        }
    }

    out
}

/// True when `email` has previously bounced and should be suppressed (Issue #484).
pub async fn is_bounced(pool: &sqlx::PgPool, email: &str) -> bool {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM email_bounces WHERE email = $1")
        .bind(email)
        .fetch_one(pool)
        .await
        .map(|c| c > 0)
        .unwrap_or(false)
}

/// Record a bounced address. Idempotent on email — a repeated bounce refreshes
/// the stored reason, provider and timestamp.
pub async fn record_bounce(
    pool: &sqlx::PgPool,
    recipient: &BouncedRecipient,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO email_bounces (email, reason, provider) VALUES ($1, $2, $3) \
         ON CONFLICT (email) DO UPDATE SET reason = EXCLUDED.reason, \
         provider = EXCLUDED.provider, bounced_at = NOW()",
    )
    .bind(&recipient.email)
    .bind(&recipient.reason)
    .bind(&recipient.provider)
    .execute(pool)
    .await?;
    Ok(())
}

/// Batched email notification sender.
/// Collects events for up to 1 minute, then sends a single summary email.
pub struct EmailNotifier {
    smtp_host: String,
    smtp_port: u16,
    smtp_user: Option<String>,
    smtp_password: Option<SecretString>,
    from: String,
    to: Vec<String>,
    contract_filter: Vec<String>,
    retry_policy: RetryPolicy,
    pool: sqlx::PgPool,
}

impl EmailNotifier {
    pub fn new(
        smtp_host: String,
        smtp_port: u16,
        smtp_user: Option<String>,
        smtp_password: Option<SecretString>,
        from: String,
        to: Vec<String>,
        contract_filter: Vec<String>,
        retry_policy: RetryPolicy,
        pool: sqlx::PgPool,
    ) -> Self {
        Self {
            smtp_host,
            smtp_port,
            smtp_user,
            smtp_password,
            from,
            to,
            contract_filter,
            retry_policy,
            pool,
        }
    }

    /// Spawn a background task that batches events and sends emails every minute.
    pub fn spawn(
        self,
        mut event_rx: tokio::sync::broadcast::Receiver<SorobanEvent>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut batch_interval = interval(Duration::from_secs(60));
            batch_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            let mut events_buffer: Vec<SorobanEvent> = Vec::new();

            loop {
                tokio::select! {
                    _ = batch_interval.tick() => {
                        if !events_buffer.is_empty() {
                            self.send_batch_email(&events_buffer).await;
                            events_buffer.clear();
                        }
                    }
                    result = event_rx.recv() => {
                        match result {
                            Ok(event) => {
                                // Apply contract filter if configured
                                if !self.contract_filter.is_empty()
                                    && !self.contract_filter.contains(&event.contract_id)
                                {
                                    continue;
                                }
                                events_buffer.push(event);
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                warn!(
                                    skipped = n,
                                    "Email notifier lagged, some events skipped"
                                );
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                // Channel closed, send any remaining events and exit
                                if !events_buffer.is_empty() {
                                    self.send_batch_email(&events_buffer).await;
                                }
                                break;
                            }
                        }
                    }
                }
            }
        })
    }

    /// Send a summary email for a batch of events with idempotency (Issue #474).
    async fn send_batch_email(&self, events: &[SorobanEvent]) {
        if events.is_empty() {
            return;
        }

        // Generate idempotency key based on event batch
        let event_ids: Vec<String> = events.iter().map(|e| e.id.to_string()).collect();
        let idempotency_key = format!("batch_{}", 
            sha2::Sha256::digest(event_ids.join(",").as_bytes())
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>()[..16].to_string()
        );

        // Check if already sent
        if let Ok(existing) = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM email_notifications WHERE idempotency_key = $1"
        )
        .bind(&idempotency_key)
        .fetch_one(&self.pool)
        .await
        {
            if existing > 0 {
                info!(idempotency_key = %idempotency_key, "Email already sent, skipping");
                return;
            }
        }

        // Group events by contract ID for better readability
        let mut by_contract: HashMap<String, Vec<&SorobanEvent>> = HashMap::new();
        for event in events {
            by_contract
                .entry(event.contract_id.clone())
                .or_default()
                .push(event);
        }

        let subject = format!(
            "Soroban Pulse: {} new event{} indexed",
            events.len(),
            if events.len() == 1 { "" } else { "s" }
        );

        let mut body = String::new();
        body.push_str(&format!(
            "Soroban Pulse indexed {} new event{} in the last minute.\n\n",
            events.len(),
            if events.len() == 1 { "" } else { "s" }
        ));

        for (contract_id, contract_events) in by_contract.iter() {
            body.push_str(&format!(
                "Contract: {}\n  Events: {}\n",
                contract_id,
                contract_events.len()
            ));

            for event in contract_events.iter().take(10) {
                body.push_str(&format!(
                    "  - Type: {}, Ledger: {}, TxHash: {}\n",
                    event.event_type, event.ledger, event.tx_hash
                ));
            }

            if contract_events.len() > 10 {
                body.push_str(&format!(
                    "  ... and {} more event{}\n",
                    contract_events.len() - 10,
                    if contract_events.len() - 10 == 1 {
                        ""
                    } else {
                        "s"
                    }
                ));
            }
            body.push('\n');
        }

        // Suppress addresses that have previously bounced (Issue #484) to
        // protect sender reputation and avoid wasting SMTP resources.
        let mut active_recipients: Vec<String> = Vec::new();
        for recipient in &self.to {
            if is_bounced(&self.pool, recipient).await {
                info!(recipient = %recipient, "Skipping previously bounced recipient");
            } else {
                active_recipients.push(recipient.clone());
            }
        }
        if active_recipients.is_empty() {
            warn!("All configured email recipients have bounced; skipping send");
            return;
        }

        // Build and send email
        if let Err(e) = self.send_email(&active_recipients, &subject, &body).await {
            error!(error = %e, "Failed to send email notification");
            metrics::record_email_failure();
        } else {
            info!(
                recipients = active_recipients.len(),
                event_count = events.len(),
                "Email notification sent successfully"
            );
        }
    }

    /// Send an email to the given recipients using SMTP.
    async fn send_email(
        &self,
        recipients: &[String],
        subject: &str,
        body: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Build message with all recipients
        let mut message_builder = Message::builder().from(self.from.parse()?).subject(subject);

        for recipient in recipients {
            message_builder = message_builder.to(recipient.parse()?);
        }

        let message = message_builder
            .header(header::ContentType::TEXT_PLAIN)
            .body(body.to_string())?;

        // Build SMTP transport
        let mut transport_builder = SmtpTransport::relay(&self.smtp_host)?.port(self.smtp_port);

        if let (Some(user), Some(password)) = (&self.smtp_user, &self.smtp_password) {
            transport_builder = transport_builder.credentials(Credentials::new(
                user.clone(),
                password.expose_secret().clone(),
            ));
        }

        let mailer = transport_builder.build();

        // Send email (blocking operation, run in spawn_blocking)
        let result = tokio::task::spawn_blocking(move || mailer.send(&message)).await?;

        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(Box::new(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mock_event(contract_id: &str, ledger: u64) -> SorobanEvent {
        SorobanEvent {
            contract_id: contract_id.to_string(),
            event_type: "contract".to_string(),
            tx_hash: "abc123".to_string(),
            ledger,
            ledger_closed_at: "2026-04-28T00:00:00Z".to_string(),
            ledger_hash: None,
            in_successful_call: true,
            value: json!({"test": "data"}),
            topic: None,
        }
    }

    #[test]
    fn test_email_notifier_creation() {
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/unused").unwrap();
        let notifier = EmailNotifier::new(
            "smtp.example.com".to_string(),
            587,
            Some("user".to_string()),
            Some(SecretString::new("pass".to_string())),
            "from@example.com".to_string(),
            vec!["to@example.com".to_string()],
            vec![],
            RetryPolicy::default(),
            pool,
        );

        assert_eq!(notifier.smtp_host, "smtp.example.com");
        assert_eq!(notifier.smtp_port, 587);
        assert_eq!(notifier.from, "from@example.com");
        assert_eq!(notifier.to.len(), 1);
    }

    #[test]
    fn test_extract_bounced_sendgrid() {
        let payload = json!([
            {"email": "a@example.com", "event": "bounce", "reason": "550 5.1.1"},
            {"email": "b@example.com", "event": "delivered"},
            {"email": "c@example.com", "event": "dropped"}
        ]);
        let bounced = extract_bounced_recipients(&payload);
        assert_eq!(bounced.len(), 2);
        assert_eq!(bounced[0].email, "a@example.com");
        assert_eq!(bounced[0].provider, "sendgrid");
        assert_eq!(bounced[0].reason.as_deref(), Some("550 5.1.1"));
        assert_eq!(bounced[1].email, "c@example.com");
    }

    #[test]
    fn test_extract_bounced_ses_direct() {
        let payload = json!({
            "notificationType": "Bounce",
            "bounce": {
                "bouncedRecipients": [
                    {"emailAddress": "x@example.com", "diagnosticCode": "smtp; 550"}
                ]
            }
        });
        let bounced = extract_bounced_recipients(&payload);
        assert_eq!(bounced.len(), 1);
        assert_eq!(bounced[0].email, "x@example.com");
        assert_eq!(bounced[0].provider, "ses");
        assert_eq!(bounced[0].reason.as_deref(), Some("smtp; 550"));
    }

    #[test]
    fn test_extract_bounced_ses_via_sns() {
        let inner = json!({
            "notificationType": "Bounce",
            "bounce": {"bouncedRecipients": [{"emailAddress": "y@example.com"}]}
        })
        .to_string();
        let payload = json!({"Type": "Notification", "Message": inner});
        let bounced = extract_bounced_recipients(&payload);
        assert_eq!(bounced.len(), 1);
        assert_eq!(bounced[0].email, "y@example.com");
        assert_eq!(bounced[0].provider, "ses");
    }

    #[test]
    fn test_extract_bounced_mailgun() {
        let modern = json!({"event-data": {"event": "failed", "recipient": "m@example.com", "reason": "bounce"}});
        let bounced = extract_bounced_recipients(&modern);
        assert_eq!(bounced.len(), 1);
        assert_eq!(bounced[0].email, "m@example.com");
        assert_eq!(bounced[0].provider, "mailgun");

        let legacy = json!({"event": "bounced", "recipient": "old@example.com"});
        let bounced = extract_bounced_recipients(&legacy);
        assert_eq!(bounced.len(), 1);
        assert_eq!(bounced[0].email, "old@example.com");
    }

    #[test]
    fn test_extract_bounced_unknown_payload() {
        let payload = json!({"hello": "world"});
        assert!(extract_bounced_recipients(&payload).is_empty());
    }

    #[test]
    fn test_secret_string_redacted_in_debug() {
        let secret = SecretString::new("my_password".to_string());
        let debug_str = format!("{:?}", secret);
        assert!(!debug_str.contains("my_password"));
        assert!(debug_str.contains("[REDACTED]"));
    }

    #[test]
    fn test_contract_filter_logic() {
        let filter = vec!["CONTRACT_A".to_string(), "CONTRACT_B".to_string()];

        let event_a = mock_event("CONTRACT_A", 100);
        let event_b = mock_event("CONTRACT_B", 101);
        let event_c = mock_event("CONTRACT_C", 102);

        assert!(filter.contains(&event_a.contract_id));
        assert!(filter.contains(&event_b.contract_id));
        assert!(!filter.contains(&event_c.contract_id));
    }

    #[test]
    fn test_empty_contract_filter_allows_all() {
        let filter: Vec<String> = vec![];
        let event = mock_event("ANY_CONTRACT", 100);

        // Empty filter means all events pass
        assert!(filter.is_empty() || filter.contains(&event.contract_id));
    }
}
