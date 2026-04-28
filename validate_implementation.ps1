# Email Notification Feature Validation Script
# This script validates that all required files and changes are in place

Write-Host "=========================================="
Write-Host "Email Notification Feature Validation"
Write-Host "=========================================="
Write-Host ""

$allChecks = @()
$passedChecks = 0
$failedChecks = 0

function Test-FileExists {
    param($path, $description)
    if (Test-Path $path) {
        Write-Host "✅ $description" -ForegroundColor Green
        $script:passedChecks++
        return $true
    } else {
        Write-Host "❌ $description" -ForegroundColor Red
        $script:failedChecks++
        return $false
    }
}

function Test-FileContains {
    param($path, $pattern, $description)
    if (Test-Path $path) {
        $content = Get-Content $path -Raw
        if ($content -match $pattern) {
            Write-Host "✅ $description" -ForegroundColor Green
            $script:passedChecks++
            return $true
        } else {
            Write-Host "❌ $description" -ForegroundColor Red
            $script:failedChecks++
            return $false
        }
    } else {
        Write-Host "❌ $description (file not found)" -ForegroundColor Red
        $script:failedChecks++
        return $false
    }
}

Write-Host "Checking Files..." -ForegroundColor Cyan
Write-Host ""

# Check new files
Test-FileExists "src/email.rs" "Email module created"
Test-FileExists "tests/email_notification_tests.rs" "Email tests created"
Test-FileExists "docs/email-notifications.md" "Email documentation created"
Test-FileExists "docs/email-quick-start.md" "Quick start guide created"
Test-FileExists "IMPLEMENTATION_SUMMARY.md" "Implementation summary created"
Test-FileExists "EMAIL_FEATURE_COMPLETE.md" "Feature completion doc created"

Write-Host ""
Write-Host "Checking Dependencies..." -ForegroundColor Cyan
Write-Host ""

# Check Cargo.toml
Test-FileContains "Cargo.toml" "lettre" "lettre dependency added to Cargo.toml"

Write-Host ""
Write-Host "Checking Configuration..." -ForegroundColor Cyan
Write-Host ""

# Check config.rs
Test-FileContains "src/config.rs" "email_smtp_host" "email_smtp_host field in Config"
Test-FileContains "src/config.rs" "email_smtp_port" "email_smtp_port field in Config"
Test-FileContains "src/config.rs" "email_smtp_user" "email_smtp_user field in Config"
Test-FileContains "src/config.rs" "email_smtp_password" "email_smtp_password field in Config"
Test-FileContains "src/config.rs" "email_from" "email_from field in Config"
Test-FileContains "src/config.rs" "email_to" "email_to field in Config"
Test-FileContains "src/config.rs" "email_contract_filter" "email_contract_filter field in Config"

Write-Host ""
Write-Host "Checking Main Integration..." -ForegroundColor Cyan
Write-Host ""

# Check main.rs
Test-FileContains "src/main.rs" "mod email" "email module imported in main.rs"
Test-FileContains "src/main.rs" "email::EmailNotifier" "EmailNotifier used in main.rs"
Test-FileContains "src/main.rs" "email_smtp_host" "email configuration checked in main.rs"

Write-Host ""
Write-Host "Checking Metrics..." -ForegroundColor Cyan
Write-Host ""

# Check metrics.rs
Test-FileContains "src/metrics.rs" "record_email_failure" "record_email_failure function added"
Test-FileContains "src/metrics.rs" "soroban_pulse_email_failures_total" "email failure metric defined"

Write-Host ""
Write-Host "Checking Module Exports..." -ForegroundColor Cyan
Write-Host ""

# Check lib.rs
Test-FileContains "src/lib.rs" "pub mod email" "email module exported in lib.rs"

Write-Host ""
Write-Host "Checking Documentation..." -ForegroundColor Cyan
Write-Host ""

# Check .env.example
Test-FileContains ".env.example" "EMAIL_SMTP_HOST" "EMAIL_SMTP_HOST documented"
Test-FileContains ".env.example" "EMAIL_SMTP_PORT" "EMAIL_SMTP_PORT documented"
Test-FileContains ".env.example" "EMAIL_SMTP_USER" "EMAIL_SMTP_USER documented"
Test-FileContains ".env.example" "EMAIL_SMTP_PASSWORD" "EMAIL_SMTP_PASSWORD documented"
Test-FileContains ".env.example" "EMAIL_FROM" "EMAIL_FROM documented"
Test-FileContains ".env.example" "EMAIL_TO" "EMAIL_TO documented"
Test-FileContains ".env.example" "EMAIL_CONTRACT_FILTER" "EMAIL_CONTRACT_FILTER documented"

# Check README.md
Test-FileContains "README.md" "Email Notifications" "Email notifications section in README"
Test-FileContains "README.md" "soroban_pulse_email_failures_total" "Email metric in README"

# Check CHANGELOG.md
Test-FileContains "CHANGELOG.md" "Email notification" "Email feature in CHANGELOG"

Write-Host ""
Write-Host "Checking Email Module Implementation..." -ForegroundColor Cyan
Write-Host ""

# Check email.rs implementation
Test-FileContains "src/email.rs" "EmailNotifier" "EmailNotifier struct defined"
Test-FileContains "src/email.rs" "spawn" "spawn method implemented"
Test-FileContains "src/email.rs" "send_batch_email" "send_batch_email method implemented"
Test-FileContains "src/email.rs" "send_email" "send_email method implemented"
Test-FileContains "src/email.rs" "lettre" "lettre crate used"
Test-FileContains "src/email.rs" "SmtpTransport" "SMTP transport configured"

Write-Host ""
Write-Host "Checking Tests..." -ForegroundColor Cyan
Write-Host ""

# Check test file
Test-FileContains "tests/email_notification_tests.rs" "test_email_notifier_filters_by_contract" "Contract filter test"
Test-FileContains "tests/email_notification_tests.rs" "test_email_notifier_accepts_all_when_no_filter" "No filter test"
Test-FileContains "tests/email_notification_tests.rs" "test_email_notifier_handles_channel_close" "Channel close test"
Test-FileContains "tests/email_notification_tests.rs" "test_email_config_parsing" "Config parsing test"

Write-Host ""
Write-Host "=========================================="
Write-Host "Validation Summary"
Write-Host "=========================================="
Write-Host ""
Write-Host "Passed: $passedChecks" -ForegroundColor Green
Write-Host "Failed: $failedChecks" -ForegroundColor Red
Write-Host ""

if ($failedChecks -eq 0) {
    Write-Host "🎉 All validation checks passed!" -ForegroundColor Green
    Write-Host ""
    Write-Host "The email notification feature is fully implemented and ready for testing."
    Write-Host ""
    Write-Host "Next steps:"
    Write-Host "1. Run 'cargo test email' to execute the test suite"
    Write-Host "2. Run 'cargo build' to compile the project"
    Write-Host "3. Configure email settings in .env"
    Write-Host "4. Start the service and verify email notifications"
    exit 0
} else {
    Write-Host "⚠️  Some validation checks failed. Please review the output above." -ForegroundColor Yellow
    exit 1
}
