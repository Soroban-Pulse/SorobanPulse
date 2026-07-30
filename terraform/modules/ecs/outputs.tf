# =============================================================================
# Module: ecs — Outputs (Issue #833)
# =============================================================================

output "cluster_id" {
  description = "ID of the ECS cluster."
  value       = aws_ecs_cluster.main.id
}

output "cluster_arn" {
  description = "ARN of the ECS cluster."
  value       = aws_ecs_cluster.main.arn
}

output "cluster_name" {
  description = "Name of the ECS cluster."
  value       = aws_ecs_cluster.main.name
}

output "service_name" {
  description = "Name of the ECS service."
  value       = aws_ecs_service.app.name
}

output "service_id" {
  description = "ID of the ECS service."
  value       = aws_ecs_service.app.id
}

output "task_definition_arn" {
  description = "ARN of the current task definition."
  value       = aws_ecs_task_definition.app.arn
}

output "task_execution_role_arn" {
  description = "ARN of the ECS task execution IAM role."
  value       = aws_iam_role.execution.arn
}

output "task_role_arn" {
  description = "ARN of the ECS task IAM role."
  value       = aws_iam_role.task.arn
}

output "log_group_name" {
  description = "CloudWatch log group name for ECS task logs."
  value       = aws_cloudwatch_log_group.ecs.name
}
