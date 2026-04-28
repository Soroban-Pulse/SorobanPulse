# Email Notifications - Quick Start

## Minimal Configuration

```bash
EMAIL_SMTP_HOST=smtp.example.com
EMAIL_FROM=soroban-pulse@example.com
EMAIL_TO=admin@example.com
```

## Common Providers

### Gmail
```bash
EMAIL_SMTP_HOST=smtp.gmail.com
EMAIL_SMTP_PORT=587
EMAIL_SMTP_USER=your-email@gmail.com
EMAIL_SMTP_PASSWORD=your-app-password  # Use App Password, not regular password
EMAIL_FROM=your-email@gmail.com
EMAIL_TO=recipient@example.com
```

### SendGrid
```bash
EMAIL_SMTP_HOST=smtp.sendgrid.net
EMAIL_SMTP_PORT=587
EMAIL_SMTP_USER=apikey
EMAIL_SMTP_PASSWORD=SG.your-api-key
EMAIL_FROM=noreply@yourdomain.com
EMAIL_TO=admin@example.com
```

### AWS SES
```bash
EMAIL_SMTP_HOST=email-smtp.us-east-1.amazonaws.com
EMAIL_SMTP_PORT=587
EMAIL_SMTP_USER=your-ses-smtp-username
EMAIL_SMTP_PASSWORD=your-ses-smtp-password
EMAIL_FROM=verified-sender@yourdomain.com
EMAIL_TO=admin@example.com
```

### Mailgun
```bash
EMAIL_SMTP_HOST=smtp.mailgun.org
EMAIL_SMTP_PORT=587
EMAIL_SMTP_USER=postmaster@yourdomain.mailgun.org
EMAIL_SMTP_PASSWORD=your-mailgun-smtp-password
EMAIL_FROM=noreply@yourdomain.com
EMAIL_TO=admin@example.com
```

## Filter by Contract

Only receive emails for specific contracts:

```bash
EMAIL_CONTRACT_FILTER=CABC123...,CDEF456...
```

## Multiple Recipients

Send to multiple email addresses:

```bash
EMAIL_TO=admin@example.com,alerts@example.com,ops@example.com
```

## Verify Configuration

Check logs on startup for:
```
Email notifications enabled smtp_host=smtp.example.com recipients=2
```

## Monitor Failures

Watch the Prometheus metric:
```
soroban_pulse_email_failures_total
```

## Troubleshooting

| Issue | Solution |
|-------|----------|
| No emails sent | Verify `EMAIL_SMTP_HOST`, `EMAIL_FROM`, and `EMAIL_TO` are all set |
| Authentication failed | Check username/password, use app-specific passwords for Gmail |
| Emails delayed | Expected - emails are batched and sent once per minute |
| Wrong recipients | Check `EMAIL_TO` for typos, ensure comma-separated |

## Test Your Configuration

1. Start the service with email configuration
2. Trigger an event (or wait for indexing)
3. Wait up to 60 seconds for the batch to send
4. Check recipient inbox and spam folder
5. Monitor logs for "Email notification sent successfully"

## Disable Email Notifications

Remove or comment out `EMAIL_SMTP_HOST`:

```bash
# EMAIL_SMTP_HOST=smtp.example.com
```

Restart the service.
