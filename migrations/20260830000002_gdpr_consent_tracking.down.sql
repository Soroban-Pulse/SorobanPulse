-- Rollback GDPR consent and data subject request tracking
DROP FUNCTION IF EXISTS gdpr_get_subject_data(TEXT);
DROP TABLE IF EXISTS gdpr_breach_notifications;
DROP TABLE IF EXISTS gdpr_data_subject_requests;
DROP TABLE IF EXISTS gdpr_consent_records;
