# Handler Split Plan

Split the large handler module by domain:

- `events`
- `subscriptions`
- `webhooks`
- `exports`
- `health`
- `admin`

Each module should own request extraction, domain service calls, and response shaping for its routes.
