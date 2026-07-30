# =============================================================================
# Root Outputs — SorobanPulse Terraform (Issue #650, #833)
# =============================================================================

# ---------------------------------------------------------------------------
# Networking
# ---------------------------------------------------------------------------

output "vpc_id" {
  description = "ID of the VPC."
  value       = module.vpc.vpc_id
}

output "public_subnet_ids" {
  description = "IDs of the public subnets (used by the ALB)."
  value       = module.vpc.public_subnet_ids
}

output "private_subnet_ids" {
  description = "IDs of the private subnets (used by RDS and the app)."
  value       = module.vpc.private_subnet_ids
}

# ---------------------------------------------------------------------------
# RDS
# ---------------------------------------------------------------------------

output "db_instance_id" {
  description = "RDS instance identifier."
  value       = module.rds.db_instance_id
}

output "db_endpoint" {
  description = "RDS connection endpoint (host:port)."
  value       = module.rds.db_endpoint
  sensitive   = true
}

output "db_name" {
  description = "Name of the PostgreSQL database."
  value       = module.rds.db_name
}

output "db_secret_arn" {
  description = "ARN of the Secrets Manager secret holding the DB password."
  value       = module.rds.db_secret_arn
  sensitive   = true
}

# ---------------------------------------------------------------------------
# ALB
# ---------------------------------------------------------------------------

output "alb_dns_name" {
  description = "DNS name of the Application Load Balancer."
  value       = module.alb.alb_dns_name
}

output "alb_zone_id" {
  description = "Route53 hosted zone ID of the ALB (for alias records)."
  value       = module.alb.alb_zone_id
}

output "alb_arn" {
  description = "ARN of the Application Load Balancer."
  value       = module.alb.alb_arn
}

output "target_group_arn" {
  description = "ARN of the ALB target group."
  value       = module.alb.target_group_arn
}

# ---------------------------------------------------------------------------
# ECS (Issue #833)
# ---------------------------------------------------------------------------

output "ecs_cluster_name" {
  description = "Name of the ECS Fargate cluster."
  value       = module.ecs.cluster_name
}

output "ecs_service_name" {
  description = "Name of the ECS service running the application."
  value       = module.ecs.service_name
}

output "ecs_task_definition_arn" {
  description = "ARN of the current ECS task definition."
  value       = module.ecs.task_definition_arn
}

# ---------------------------------------------------------------------------
# Backup (Issue #833)
# ---------------------------------------------------------------------------

output "backup_bucket_id" {
  description = "Name of the S3 bucket used for automated backups."
  value       = module.backup.bucket_id
}

output "backup_bucket_arn" {
  description = "ARN of the S3 backup bucket."
  value       = module.backup.bucket_arn
}

# ---------------------------------------------------------------------------
# Monitoring
# ---------------------------------------------------------------------------

output "log_group_name" {
  description = "CloudWatch log group name for the application."
  value       = module.monitoring.log_group_name
}

output "sns_alarm_topic_arn" {
  description = "ARN of the SNS topic used for CloudWatch alarm notifications."
  value       = module.monitoring.sns_alarm_topic_arn
}
