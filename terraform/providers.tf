# =============================================================================
# Provider Configuration — SorobanPulse (Issue #650)
#
# Configures the AWS provider and Terraform backend settings.
# All resources are created in the configured AWS region.
# =============================================================================

terraform {
  required_version = ">= 1.6.0"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.73"
    }
    random = {
      source  = "hashicorp/random"
      version = "~> 3.6"
    }
  }

  # Remote state: use an S3 bucket + DynamoDB lock table.
  # Replace the bucket name and table name for your account before first use.
  # See docs/terraform.md for bootstrap instructions.
  backend "s3" {
    bucket         = "soroban-pulse-terraform-state"
    key            = "soroban-pulse/terraform.tfstate"
    region         = "us-east-1"
    encrypt        = true
    dynamodb_table = "soroban-pulse-terraform-locks"
  }
}

provider "aws" {
  region = var.aws_region

  default_tags {
    tags = {
      Project     = "SorobanPulse"
      ManagedBy   = "Terraform"
      Environment = var.environment
    }
  }
}
