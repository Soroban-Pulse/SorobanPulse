# =============================================================================
# Module: backup — Automated Backup Storage (Issue #833)
#
# Creates:
#   - S3 bucket for database and application backups
#   - Lifecycle rules for tiered storage and expiration
#   - Bucket versioning for point-in-time recovery
#   - Server-side encryption (AES-256)
#   - Bucket policy blocking public access
# =============================================================================

# ---------------------------------------------------------------------------
# S3 Bucket
# ---------------------------------------------------------------------------

resource "aws_s3_bucket" "backups" {
  bucket        = "${var.name_prefix}-backups"
  force_destroy = var.force_destroy

  tags = {
    Name    = "${var.name_prefix}-backups"
    Purpose = "Automated backup storage"
  }
}

# ---------------------------------------------------------------------------
# Block Public Access
# ---------------------------------------------------------------------------

resource "aws_s3_bucket_public_access_block" "backups" {
  bucket = aws_s3_bucket.backups.id

  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

# ---------------------------------------------------------------------------
# Versioning
# ---------------------------------------------------------------------------

resource "aws_s3_bucket_versioning" "backups" {
  bucket = aws_s3_bucket.backups.id

  versioning_configuration {
    status = "Enabled"
  }
}

# ---------------------------------------------------------------------------
# Server-Side Encryption
# ---------------------------------------------------------------------------

resource "aws_s3_bucket_server_side_encryption_configuration" "backups" {
  bucket = aws_s3_bucket.backups.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "aws:kms"
    }
    bucket_key_enabled = true
  }
}

# ---------------------------------------------------------------------------
# Lifecycle Rules
# ---------------------------------------------------------------------------

resource "aws_s3_bucket_lifecycle_configuration" "backups" {
  bucket = aws_s3_bucket.backups.id

  # Move current backups to Infrequent Access after 30 days
  rule {
    id     = "transition-to-ia"
    status = "Enabled"

    filter {
      prefix = "db/"
    }

    transition {
      days          = 30
      storage_class = "STANDARD_IA"
    }
  }

  # Move older backups to Glacier after 90 days
  rule {
    id     = "transition-to-glacier"
    status = "Enabled"

    filter {
      prefix = "db/"
    }

    transition {
      days          = 90
      storage_class = "GLACIER"
    }
  }

  # Expire backups after the retention period
  rule {
    id     = "expire-old-backups"
    status = "Enabled"

    filter {
      prefix = "db/"
    }

    expiration {
      days = var.retention_days
    }
  }

  # Clean up incomplete multipart uploads
  rule {
    id     = "abort-incomplete-uploads"
    status = "Enabled"

    filter {
      prefix = ""
    }

    abort_incomplete_multipart_upload {
      days_after_initiation = 7
    }
  }

  # Expire old versions of objects
  rule {
    id     = "expire-old-versions"
    status = "Enabled"

    filter {
      prefix = ""
    }

    noncurrent_version_expiration {
      noncurrent_days = var.version_retention_days
    }
  }
}

# ---------------------------------------------------------------------------
# Bucket Policy — enforce encryption in transit
# ---------------------------------------------------------------------------

resource "aws_s3_bucket_policy" "backups" {
  bucket = aws_s3_bucket.backups.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid       = "DenyUnencryptedTransport"
        Effect    = "Deny"
        Principal = "*"
        Action    = "s3:*"
        Resource = [
          aws_s3_bucket.backups.arn,
          "${aws_s3_bucket.backups.arn}/*"
        ]
        Condition = {
          Bool = {
            "aws:SecureTransport" = "false"
          }
        }
      }
    ]
  })
}
