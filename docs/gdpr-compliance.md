# GDPR Data Handling Compliance (Issue #945)

This document describes SorobanPulse's GDPR compliance procedures.

## Scope

SorobanPulse indexes **publicly available on-chain data** from the Stellar network. Blockchain events are not personal data under GDPR. However, the following data SorobanPulse collects **does** fall under GDPR:

| Data | Table | Article 4 Classification |
|---|---|---|
| Subscriber email addresses | `subscriptions` | Personal data |
| Webhook endpoint URLs | `webhooks` | Potentially personal |
| Email delivery logs | `email_deliveries`, `notification_audit_log` | Personal data |
| Consent records | `gdpr_consent_records` | Sensitive personal data |
| IP addresses in audit logs | `audit_logs` | Personal data |

## Legal Basis for Processing (Article 6)

| Processing Activity | Legal Basis |
|---|---|
| Sending event notifications | Contract (subscription agreement) |
| Delivery logging | Legitimate interest (service quality) |
| Audit logging | Legal obligation |
| Analytics | Consent |

## Data Subject Rights

### Right of Access (Article 15)

Export all personal data for a subject:

```sql
SELECT gdpr_get_subject_data('user@example.com');
```

Or via the Rust API:

```rust
use soroban_pulse::gdpr::export_subject_data;

let data = export_subject_data(&pool, "user@example.com").await?;
```

Respond to the data subject within **30 days**.

### Right to Erasure (Article 17)

```rust
use soroban_pulse::gdpr::{execute_erasure_request, create_data_subject_request, DsrType};

// Register the request
let dsr_id = create_data_subject_request(
    &pool, "user@example.com", DsrType::Erasure, None
).await?;

// Execute the erasure
let result = execute_erasure_request(&pool, "user@example.com").await?;
println!("Deleted {} subscriptions", result.subscriptions_deleted);
println!("Deleted {} email deliveries", result.email_deliveries_deleted);

// Mark request complete
use soroban_pulse::gdpr::{update_dsr_status, DsrStatus};
update_dsr_status(&pool, &dsr_id, DsrStatus::Completed, Some("ops-team"), None).await?;
```

> **Note:** Contract events on the Stellar ledger are immutable public blockchain data. They cannot be erased. Inform data subjects of this limitation.

### Right to Portability (Article 20)

```sql
SELECT gdpr_get_subject_data('user@example.com');
```

Return the JSON response to the data subject within 30 days.

### Right to Rectification (Article 16)

```sql
UPDATE subscriptions SET email = 'new@example.com' WHERE email = 'old@example.com';
UPDATE email_deliveries SET recipient = 'new@example.com' WHERE recipient = 'old@example.com';
```

## Consent Tracking

Record consent grants and withdrawals:

```rust
use soroban_pulse::gdpr::{ConsentRecord, ConsentType, LegalBasis, record_consent};

let record = ConsentRecord {
    subject_email: "user@example.com".to_string(),
    consent_type: ConsentType::Notifications,
    granted: true,
    legal_basis: LegalBasis::Consent,
    source: "signup_form".to_string(),
    ip_address: Some("203.0.113.1".to_string()),
    notes: None,
};

record_consent(&pool, &record).await?;
```

## Data Subject Request Workflow

1. Data subject submits request (email, web form, or API)
2. Operator creates DSR record: `create_data_subject_request()`
3. System processes the request within **30 days**
4. Operator executes action and updates status: `update_dsr_status()`
5. Confirmation is sent to the data subject

### Overdue DSR Query

```sql
SELECT id, subject_email, request_type, requested_at, deadline_at
FROM gdpr_data_subject_requests
WHERE status NOT IN ('completed', 'rejected')
  AND deadline_at < NOW()
ORDER BY deadline_at;
```

## Breach Notification Procedure (Articles 33 & 34)

**72-hour rule**: The supervisory authority must be notified within 72 hours of detecting a breach.

```rust
use soroban_pulse::gdpr::{BreachNotification, record_breach, mark_authority_notified};

// Step 1: Record the breach immediately on detection
let breach = BreachNotification {
    detected_at: Utc::now(),
    breach_type: "confidentiality".to_string(),
    data_categories: vec!["email_addresses".to_string()],
    affected_subject_count: Some(50),
    description: "Subscription database accessed by unauthorized party".to_string(),
    containment_measures: Some("Database credentials rotated, access revoked".to_string()),
    likely_consequences: Some("Low risk: email addresses only, no financial data".to_string()),
};

let breach_id = record_breach(&pool, &breach).await?;

// Step 2: Notify the supervisory authority (within 72 hours)
mark_authority_notified(&pool, &breach_id, Some("ICO-2026-1234")).await?;
```

## Privacy Impact Assessment

A PIA should be conducted before:
- Introducing new categories of personal data processing
- Implementing automated decision-making
- Deploying new third-party integrations that receive personal data

Document PIA results in your organisation's compliance management system and reference them in this document.

## Data Processing Agreements

If you operate SorobanPulse as a SaaS on behalf of customers:

1. You are the **data processor**; your customers are **data controllers**.
2. Execute a signed DPA with each customer before processing their personal data.
3. Ensure your infrastructure providers (AWS, GCP, SMTP relay) also have DPAs.
4. Maintain a Record of Processing Activities (RoPA) per Article 30.

## Retention Periods

See [Data Retention Policy](data-retention.md) for full details.

| Data | Retention | Legal Basis |
|---|---|---|
| Subscription records | Until erasure requested | Contract |
| Email delivery logs | 90 days | Legitimate interest |
| Consent records | Duration of relationship + 3 years | Legal obligation |
| Breach notifications | 5 years | Legal obligation (Article 33(5)) |
| DSR records | 3 years after completion | Legal obligation |

## See Also

- [Data Retention Policy](data-retention.md)
- [Audit Trail](audit-trail.md)
- [Encryption](encryption.md)
- [SOC 2 Compliance](soc2-compliance.md)
