# =============================================================================
# Module: backup — Outputs (Issue #833)
# =============================================================================

output "bucket_id" {
  description = "ID (name) of the backup S3 bucket."
  value       = aws_s3_bucket.backups.id
}

output "bucket_arn" {
  description = "ARN of the backup S3 bucket."
  value       = aws_s3_bucket.backups.arn
}

output "bucket_domain_name" {
  description = "Regional domain name of the backup S3 bucket."
  value       = aws_s3_bucket.backups.bucket_regional_domain_name
}
