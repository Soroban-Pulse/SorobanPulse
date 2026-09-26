# Idempotency-Key Support

POST endpoints should accept an `Idempotency-Key` header and persist request fingerprints for a bounded TTL.

Rules:

- Same key and same request fingerprint returns the original response.
- Same key with a different fingerprint returns `409 Conflict`.
- Keys expire after the configured retention window.
