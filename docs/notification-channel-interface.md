# Notification Channel Interface

Slack, Discord, Telegram, GitHub, PagerDuty, SMS, and email should implement a common channel interface:

- `validate_config`
- `render_message`
- `send`
- `health_check`
