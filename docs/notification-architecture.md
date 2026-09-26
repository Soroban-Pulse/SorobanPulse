# Notification Delivery Architecture (Issue #994)

> **Status: implementation only, not test-verified.** This change was written
> to satisfy the issue #994 checklist without running the build or test
> suite. Treat everything below as **unreviewed and unverified** until
> someone runs `cargo build` / `cargo test` and exercises real deliveries.

## Problem

Delivery logic for email (`src/email.rs`), SMS (`src/sms.rs`), and push
(`src/push_notification.rs`) each independently implemented their own retry
loop, error representation (`String`, `Box<dyn Error>`, or a bespoke enum),
and success/failure metrics recording, making it easy for the three channels
to drift out of sync — e.g. only some accounted for jitter, or emitted
different metric names on failure.

This is a distinct concern from [`crate::notification_channel`] (issue
#809), which already provides a shared `NotificationChannel` trait for
*webhook-style* outbound channels (Discord, Telegram, ...). That trait's
shape doesn't fit email/SMS/push, which need retry classification
(permanent vs. transient failure) and a delivery *target* string
(address/number/token) rather than a fixed webhook endpoint.

## Design

`src/notification_delivery.rs` introduces:

- **`DeliveryChannel` trait** — the unified interface. Each channel
  implements `channel_name()`, `retry_policy()`, and a single-attempt
  `deliver(target, subject, body)`.
- **`NotificationError` enum** — consolidated error classification shared by
  all channels:
  - `Transient` — safe to retry (network blip, upstream 5xx, rate limit).
  - `InvalidTarget` — the address/number/token is permanently bad; retrying
    won't help, and callers should stop sending to it.
  - `Configuration` — the channel itself is misconfigured (missing
    credentials); not retryable, needs operator action.
- **`DeliveryOutcome` enum** — `Delivered` or `InvalidTarget`, mirroring the
  `Ok(true)` / `Ok(false)` convention already used by
  `push_notification::fcm_send` / `apns_send` / `web_push_send`.
- **`deliver_with_retry()`** — the shared retry driver. Retries only on
  `NotificationError::Transient`, using the channel's own `RetryPolicy`
  (`src/retry_policy.rs`, already shared infra) for backoff/jitter timing.
  `InvalidTarget` and `Configuration` fail fast. Records
  `soroban_pulse_notification_delivery_{success,failure}_total` and
  `soroban_pulse_notification_delivery_latency_seconds{channel=...}`
  uniformly for every channel — previously each channel recorded these (or
  channel-specific equivalents) independently.

## Per-channel adapters

| Channel | Adapter | Notes |
|---|---|---|
| Email | `impl DeliveryChannel for EmailNotifier` (`src/email.rs`) | Delegates to the existing private `send_email`. Its underlying `lettre` error doesn't currently distinguish permanent vs. transient failure, so it's conservatively classified `Transient`. **Follow-up**: parse SMTP response codes (5xx = permanent, 4xx = transient) to classify properly. |
| SMS | `impl DeliveryChannel for SmsNotifier` (`src/sms.rs`) | Delegates to the existing private `send_twilio_sms`. Classifies Twilio error code `21211` ("invalid 'To' phone number") as `InvalidTarget`; everything else as `Transient`. **Follow-up**: parse the full Twilio error-code table instead of one hardcoded code. |
| Push | `PushChannelAdapter` (`src/push_notification.rs`) | New adapter struct (channels didn't previously share a "notifier" type); dispatches to `fcm_send`/`apns_send` by `DeviceType`, matching `run_push_delivery_worker`'s existing FCM-for-Android-and-Web routing. `Ok(false)` (invalid/expired token) maps to `DeliveryOutcome::InvalidTarget`. **Note**: true W3C Web Push (VAPID, `WebPushSubscription`) needs an endpoint + key pair, not a bare token, so `web_push_send` is not wired into this adapter — see Follow-up below. |

## What did *not* change

- `EmailNotifier::spawn`, `SmsNotifier::spawn`, and
  `run_push_delivery_worker` still contain their original send loops; they
  were **not** rewired to call `deliver_with_retry` in this pass. The
  adapters above make each channel *capable* of running through the shared
  framework, but the batching/scheduling/quiet-hours logic in each `spawn`
  method is channel-specific enough (and, in `email.rs`'s case, already
  affected by pre-existing unrelated bugs in this file) that swapping the
  call sites was judged out of scope for a no-test-run pass. Doing so is the
  natural next step once this module has been built and exercised.
- Discord/Telegram/Slack/GitHub/PagerDuty/Teams webhook channels are
  unaffected — they already share `crate::notification_channel::NotificationChannel`
  (issue #809) and are a different shape of problem (fixed endpoint,
  no per-target retry classification).

## Follow-up (explicitly out of scope for this change)

- Delivery tests were intentionally **not** written, per the instruction
  this change was implemented under ("implement only, do not add tests").
  Before relying on `deliver_with_retry` in production, add tests covering:
  transient-error retry + backoff, invalid-target fail-fast, configuration
  fail-fast, and retry exhaustion.
- Wire `EmailNotifier::spawn`/`SmsNotifier::spawn`/`run_push_delivery_worker`
  to actually call `deliver_with_retry` instead of their own inline retry
  logic, once the framework has been reviewed.
- Add proper SMTP and Twilio error-code classification (see table above).
- Extend `PushChannelAdapter` (or add a sibling) to support true VAPID Web
  Push via `WebPushSubscription`.
