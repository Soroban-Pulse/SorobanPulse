// =============================================================================
// Terraform Validation Tests (Issue #833)
//
// Validates Terraform configuration file structure and completeness by reading
// .tf files as text. These tests do NOT require Terraform to be installed.
// =============================================================================

use std::fs;
use std::path::Path;

/// Root terraform directory relative to the project root.
const TERRAFORM_DIR: &str = "terraform";

// =============================================================================
// Helpers
// =============================================================================

/// Read a Terraform file relative to the project root and return its contents.
fn read_tf_file(relative_path: &str) -> String {
    let path = Path::new(TERRAFORM_DIR).join(relative_path);
    fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "Failed to read Terraform file '{}': {}",
            path.display(),
            e
        )
    })
}

/// Assert that a file exists within the terraform directory.
fn assert_tf_file_exists(relative_path: &str) {
    let path = Path::new(TERRAFORM_DIR).join(relative_path);
    assert!(
        path.exists(),
        "Expected Terraform file does not exist: {}",
        path.display()
    );
}

/// Assert that file content contains a specific pattern.
fn assert_contains(content: &str, pattern: &str, file_name: &str) {
    assert!(
        content.contains(pattern),
        "File '{}' should contain '{}' but it was not found.\nContent:\n{}",
        file_name,
        pattern,
        &content[..content.len().min(2000)]
    );
}

// =============================================================================
// Root Configuration Tests
// =============================================================================

#[test]
fn root_main_tf_exists_and_has_required_modules() {
    assert_tf_file_exists("main.tf");
    let content = read_tf_file("main.tf");

    // All expected module blocks must be present
    assert_contains(&content, "module \"vpc\"", "main.tf");
    assert_contains(&content, "module \"rds\"", "main.tf");
    assert_contains(&content, "module \"alb\"", "main.tf");
    assert_contains(&content, "module \"monitoring\"", "main.tf");
    assert_contains(&content, "module \"ecs\"", "main.tf");
    assert_contains(&content, "module \"backup\"", "main.tf");
}

#[test]
fn root_main_tf_wires_module_sources_correctly() {
    let content = read_tf_file("main.tf");

    assert_contains(&content, "source = \"./modules/vpc\"", "main.tf");
    assert_contains(&content, "source = \"./modules/rds\"", "main.tf");
    assert_contains(&content, "source = \"./modules/alb\"", "main.tf");
    assert_contains(&content, "source = \"./modules/monitoring\"", "main.tf");
    assert_contains(&content, "source = \"./modules/ecs\"", "main.tf");
    assert_contains(&content, "source = \"./modules/backup\"", "main.tf");
}

#[test]
fn root_variables_tf_exists_and_has_required_variables() {
    assert_tf_file_exists("variables.tf");
    let content = read_tf_file("variables.tf");

    let required_vars = [
        "aws_region",
        "environment",
        "project_name",
        "vpc_cidr",
        "availability_zones",
        "public_subnet_cidrs",
        "private_subnet_cidrs",
        "db_instance_class",
        "db_name",
        "certificate_arn",
        "app_port",
        "app_container_count",
        "ecs_task_cpu",
        "ecs_task_memory",
        "backup_retention_days",
        "log_retention_days",
    ];

    for var_name in &required_vars {
        let pattern = format!("variable \"{}\"", var_name);
        assert_contains(&content, &pattern, "variables.tf");
    }
}

#[test]
fn root_variables_tf_environment_allows_dev() {
    let content = read_tf_file("variables.tf");

    // The environment variable validation should allow "dev"
    assert_contains(&content, "\"dev\"", "variables.tf");
    assert_contains(&content, "\"staging\"", "variables.tf");
    assert_contains(&content, "\"production\"", "variables.tf");
}

#[test]
fn root_outputs_tf_exists_and_has_required_outputs() {
    assert_tf_file_exists("outputs.tf");
    let content = read_tf_file("outputs.tf");

    let required_outputs = [
        "vpc_id",
        "public_subnet_ids",
        "private_subnet_ids",
        "db_instance_id",
        "db_endpoint",
        "db_name",
        "alb_dns_name",
        "alb_arn",
        "target_group_arn",
        "ecs_cluster_name",
        "ecs_service_name",
        "backup_bucket_id",
        "backup_bucket_arn",
        "log_group_name",
        "sns_alarm_topic_arn",
    ];

    for output_name in &required_outputs {
        let pattern = format!("output \"{}\"", output_name);
        assert_contains(&content, &pattern, "outputs.tf");
    }
}

#[test]
fn providers_tf_exists_and_has_backend_config() {
    assert_tf_file_exists("providers.tf");
    let content = read_tf_file("providers.tf");

    assert_contains(&content, "backend \"s3\"", "providers.tf");
    assert_contains(&content, "dynamodb_table", "providers.tf");
    assert_contains(&content, "encrypt", "providers.tf");
    assert_contains(&content, "provider \"aws\"", "providers.tf");
    assert_contains(&content, "required_version", "providers.tf");
}

// =============================================================================
// Module Structure Tests
// =============================================================================

#[test]
fn all_modules_have_required_files() {
    let modules = ["vpc", "rds", "alb", "monitoring", "ecs", "backup"];

    for module in &modules {
        let required_files = ["main.tf", "variables.tf", "outputs.tf"];
        for file in &required_files {
            let path = format!("modules/{}/{}", module, file);
            assert_tf_file_exists(&path);
        }
    }
}

// =============================================================================
// ECS Module Tests
// =============================================================================

#[test]
fn ecs_module_has_cluster_resource() {
    let content = read_tf_file("modules/ecs/main.tf");
    assert_contains(&content, "aws_ecs_cluster", "modules/ecs/main.tf");
}

#[test]
fn ecs_module_has_task_definition() {
    let content = read_tf_file("modules/ecs/main.tf");
    assert_contains(
        &content,
        "aws_ecs_task_definition",
        "modules/ecs/main.tf",
    );
    assert_contains(&content, "FARGATE", "modules/ecs/main.tf");
    assert_contains(&content, "awsvpc", "modules/ecs/main.tf");
}

#[test]
fn ecs_module_has_service() {
    let content = read_tf_file("modules/ecs/main.tf");
    assert_contains(&content, "aws_ecs_service", "modules/ecs/main.tf");
    assert_contains(&content, "load_balancer", "modules/ecs/main.tf");
}

#[test]
fn ecs_module_has_iam_roles() {
    let content = read_tf_file("modules/ecs/main.tf");
    assert_contains(&content, "aws_iam_role", "modules/ecs/main.tf");
    assert_contains(
        &content,
        "ecs-tasks.amazonaws.com",
        "modules/ecs/main.tf",
    );
}

#[test]
fn ecs_module_has_required_variables() {
    let content = read_tf_file("modules/ecs/variables.tf");

    let required_vars = [
        "name_prefix",
        "aws_region",
        "container_image",
        "task_cpu",
        "task_memory",
        "desired_count",
        "app_port",
        "private_subnet_ids",
        "security_group_ids",
        "target_group_arn",
    ];

    for var_name in &required_vars {
        let pattern = format!("variable \"{}\"", var_name);
        assert_contains(&content, &pattern, "modules/ecs/variables.tf");
    }
}

#[test]
fn ecs_module_has_required_outputs() {
    let content = read_tf_file("modules/ecs/outputs.tf");

    let required_outputs = [
        "cluster_id",
        "cluster_arn",
        "service_name",
        "task_definition_arn",
        "task_execution_role_arn",
        "task_role_arn",
    ];

    for output_name in &required_outputs {
        let pattern = format!("output \"{}\"", output_name);
        assert_contains(&content, &pattern, "modules/ecs/outputs.tf");
    }
}

// =============================================================================
// Backup Module Tests
// =============================================================================

#[test]
fn backup_module_has_s3_bucket() {
    let content = read_tf_file("modules/backup/main.tf");
    assert_contains(&content, "aws_s3_bucket", "modules/backup/main.tf");
}

#[test]
fn backup_module_has_lifecycle_rules() {
    let content = read_tf_file("modules/backup/main.tf");
    assert_contains(
        &content,
        "aws_s3_bucket_lifecycle_configuration",
        "modules/backup/main.tf",
    );
    assert_contains(&content, "STANDARD_IA", "modules/backup/main.tf");
    assert_contains(&content, "GLACIER", "modules/backup/main.tf");
}

#[test]
fn backup_module_has_encryption() {
    let content = read_tf_file("modules/backup/main.tf");
    assert_contains(
        &content,
        "aws_s3_bucket_server_side_encryption_configuration",
        "modules/backup/main.tf",
    );
}

#[test]
fn backup_module_has_versioning() {
    let content = read_tf_file("modules/backup/main.tf");
    assert_contains(
        &content,
        "aws_s3_bucket_versioning",
        "modules/backup/main.tf",
    );
}

#[test]
fn backup_module_blocks_public_access() {
    let content = read_tf_file("modules/backup/main.tf");
    assert_contains(
        &content,
        "aws_s3_bucket_public_access_block",
        "modules/backup/main.tf",
    );
}

#[test]
fn backup_module_has_required_variables() {
    let content = read_tf_file("modules/backup/variables.tf");

    let required_vars = ["name_prefix", "retention_days", "force_destroy"];

    for var_name in &required_vars {
        let pattern = format!("variable \"{}\"", var_name);
        assert_contains(&content, &pattern, "modules/backup/variables.tf");
    }
}

#[test]
fn backup_module_has_required_outputs() {
    let content = read_tf_file("modules/backup/outputs.tf");

    let required_outputs = ["bucket_id", "bucket_arn"];

    for output_name in &required_outputs {
        let pattern = format!("output \"{}\"", output_name);
        assert_contains(&content, &pattern, "modules/backup/outputs.tf");
    }
}

// =============================================================================
// Environment Configuration Tests
// =============================================================================

#[test]
fn dev_environment_exists() {
    assert_tf_file_exists("environments/dev/terraform.tfvars.example");
}

#[test]
fn staging_environment_exists() {
    assert_tf_file_exists("environments/staging/terraform.tfvars.example");
}

#[test]
fn production_environment_exists() {
    assert_tf_file_exists("environments/production/terraform.tfvars.example");
}

#[test]
fn dev_environment_has_correct_environment_value() {
    let content = read_tf_file("environments/dev/terraform.tfvars.example");
    assert_contains(
        &content,
        "environment  = \"dev\"",
        "environments/dev/terraform.tfvars.example",
    );
}

#[test]
fn all_environments_have_core_variables() {
    let environments = ["dev", "staging", "production"];

    for env in &environments {
        let path = format!("environments/{}/terraform.tfvars.example", env);
        let content = read_tf_file(&path);

        assert_contains(&content, "aws_region", &path);
        assert_contains(&content, "environment", &path);
        assert_contains(&content, "project_name", &path);
        assert_contains(&content, "vpc_cidr", &path);
        assert_contains(&content, "db_instance_class", &path);
        assert_contains(&content, "app_port", &path);
    }
}

// =============================================================================
// State Management Tests
// =============================================================================

#[test]
fn terraform_init_script_exists() {
    let path = Path::new("scripts/terraform-init.sh");
    assert!(
        path.exists(),
        "Terraform bootstrap script should exist at scripts/terraform-init.sh"
    );
}

#[test]
fn terraform_init_script_creates_s3_bucket() {
    let content =
        fs::read_to_string("scripts/terraform-init.sh").expect("Failed to read terraform-init.sh");

    assert_contains(&content, "s3api create-bucket", "terraform-init.sh");
    assert_contains(&content, "put-bucket-versioning", "terraform-init.sh");
    assert_contains(&content, "put-bucket-encryption", "terraform-init.sh");
}

#[test]
fn terraform_init_script_creates_dynamodb_table() {
    let content =
        fs::read_to_string("scripts/terraform-init.sh").expect("Failed to read terraform-init.sh");

    assert_contains(&content, "dynamodb create-table", "terraform-init.sh");
    assert_contains(&content, "LockID", "terraform-init.sh");
}

#[test]
fn terraform_init_script_validates_environment() {
    let content =
        fs::read_to_string("scripts/terraform-init.sh").expect("Failed to read terraform-init.sh");

    // Script should validate that environment is one of the allowed values
    assert_contains(&content, "dev", "terraform-init.sh");
    assert_contains(&content, "staging", "terraform-init.sh");
    assert_contains(&content, "production", "terraform-init.sh");
}

// =============================================================================
// Cross-Module Wiring Tests
// =============================================================================

#[test]
fn main_tf_passes_vpc_outputs_to_downstream_modules() {
    let content = read_tf_file("main.tf");

    // ECS should receive private subnets from VPC
    assert_contains(&content, "module.vpc.private_subnet_ids", "main.tf");

    // ECS should receive app security group from VPC
    assert_contains(&content, "module.vpc.app_security_group_id", "main.tf");

    // ALB should receive public subnets from VPC
    assert_contains(&content, "module.vpc.public_subnet_ids", "main.tf");
}

#[test]
fn main_tf_connects_ecs_to_alb_target_group() {
    let content = read_tf_file("main.tf");

    // ECS module should receive the ALB target group ARN
    assert_contains(&content, "module.alb.target_group_arn", "main.tf");
}

#[test]
fn main_tf_passes_rds_secret_to_ecs() {
    let content = read_tf_file("main.tf");

    // ECS should receive the RDS secret ARN for database credentials
    assert_contains(&content, "module.rds.db_secret_arn", "main.tf");
}

// =============================================================================
// CI Integration Tests
// =============================================================================

#[test]
fn ci_workflow_includes_terraform_validation() {
    let content = fs::read_to_string(".github/workflows/ci.yml")
        .expect("Failed to read CI workflow file");

    assert_contains(&content, "terraform-validate", "ci.yml");
    assert_contains(&content, "terraform fmt", "ci.yml");
    assert_contains(&content, "terraform validate", "ci.yml");
    assert_contains(&content, "tflint", "ci.yml");
}
