#!/usr/bin/env bash
# =============================================================================
# Terraform State Bootstrap Script (Issue #833)
#
# Creates the S3 bucket and DynamoDB table required by the Terraform backend
# before the first `terraform init`.
#
# Usage:
#   ./scripts/terraform-init.sh [ENVIRONMENT]
#
# Arguments:
#   ENVIRONMENT   dev | staging | production (default: staging)
#
# Prerequisites:
#   - AWS CLI configured with appropriate credentials
#   - Sufficient IAM permissions to create S3 buckets and DynamoDB tables
# =============================================================================

set -euo pipefail

ENVIRONMENT="${1:-staging}"
PROJECT="soroban-pulse"
REGION="${AWS_REGION:-us-east-1}"

BUCKET_NAME="${PROJECT}-terraform-state"
TABLE_NAME="${PROJECT}-terraform-locks"

echo "=== Terraform State Bootstrap ==="
echo "Environment : ${ENVIRONMENT}"
echo "Region      : ${REGION}"
echo "S3 Bucket   : ${BUCKET_NAME}"
echo "DynamoDB    : ${TABLE_NAME}"
echo ""

# ---------------------------------------------------------------------------
# Validate environment
# ---------------------------------------------------------------------------
if [[ ! "${ENVIRONMENT}" =~ ^(dev|staging|production)$ ]]; then
  echo "ERROR: ENVIRONMENT must be 'dev', 'staging', or 'production'."
  exit 1
fi

# ---------------------------------------------------------------------------
# Create S3 bucket for Terraform state
# ---------------------------------------------------------------------------
echo "--- Creating S3 state bucket (if not exists) ---"

if aws s3api head-bucket --bucket "${BUCKET_NAME}" 2>/dev/null; then
  echo "Bucket '${BUCKET_NAME}' already exists — skipping creation."
else
  aws s3api create-bucket \
    --bucket "${BUCKET_NAME}" \
    --region "${REGION}" \
    $([ "${REGION}" != "us-east-1" ] && echo "--create-bucket-configuration LocationConstraint=${REGION}")

  echo "Bucket '${BUCKET_NAME}' created."
fi

# Enable versioning (idempotent)
echo "Enabling bucket versioning..."
aws s3api put-bucket-versioning \
  --bucket "${BUCKET_NAME}" \
  --versioning-configuration Status=Enabled

# Enable server-side encryption (idempotent)
echo "Enabling bucket encryption (AES-256)..."
aws s3api put-bucket-encryption \
  --bucket "${BUCKET_NAME}" \
  --server-side-encryption-configuration '{
    "Rules": [
      {
        "ApplyServerSideEncryptionByDefault": {
          "SSEAlgorithm": "aws:kms"
        },
        "BucketKeyEnabled": true
      }
    ]
  }'

# Block public access (idempotent)
echo "Blocking public access..."
aws s3api put-public-access-block \
  --bucket "${BUCKET_NAME}" \
  --public-access-block-configuration \
    BlockPublicAcls=true,IgnorePublicAcls=true,BlockPublicPolicy=true,RestrictPublicBuckets=true

# ---------------------------------------------------------------------------
# Create DynamoDB table for state locking
# ---------------------------------------------------------------------------
echo ""
echo "--- Creating DynamoDB lock table (if not exists) ---"

if aws dynamodb describe-table --table-name "${TABLE_NAME}" --region "${REGION}" 2>/dev/null; then
  echo "Table '${TABLE_NAME}' already exists — skipping creation."
else
  aws dynamodb create-table \
    --table-name "${TABLE_NAME}" \
    --attribute-definitions AttributeName=LockID,AttributeType=S \
    --key-schema AttributeName=LockID,KeyType=HASH \
    --billing-mode PAY_PER_REQUEST \
    --region "${REGION}" \
    --tags Key=Project,Value="${PROJECT}" Key=ManagedBy,Value=Terraform Key=Purpose,Value=state-locking

  echo "Waiting for table to become ACTIVE..."
  aws dynamodb wait table-exists --table-name "${TABLE_NAME}" --region "${REGION}"
  echo "Table '${TABLE_NAME}' created and active."
fi

# ---------------------------------------------------------------------------
# Enable point-in-time recovery on the lock table
# ---------------------------------------------------------------------------
echo "Enabling point-in-time recovery on lock table..."
aws dynamodb update-continuous-backups \
  --table-name "${TABLE_NAME}" \
  --region "${REGION}" \
  --point-in-time-recovery-specification PointInTimeRecoveryEnabled=true 2>/dev/null || true

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo ""
echo "=== Bootstrap Complete ==="
echo ""
echo "You can now run:"
echo "  cd terraform"
echo "  terraform init"
echo "  terraform plan -var-file=environments/${ENVIRONMENT}/terraform.tfvars"
echo ""
echo "State will be stored in: s3://${BUCKET_NAME}/${PROJECT}/terraform.tfstate"
echo "Locks managed via DynamoDB table: ${TABLE_NAME}"
