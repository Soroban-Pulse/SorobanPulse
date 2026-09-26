#!/usr/bin/env bash
# check_secrets.sh — Detect hardcoded secrets, tokens, and credentials
#
# Usage:
#   bash scripts/check_secrets.sh            # scan the whole repository
#   bash scripts/check_secrets.sh --help     # show help
#
# Exit codes:
#   0  No issues found
#   1  Potential secrets detected (review required)
#
# This script is intentionally lightweight and grep-based so it runs without
# any additional tooling.  For more comprehensive detection, pair it with
# `truffleHog` or `detect-secrets` in CI.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# ---------------------------------------------------------------------------
# Parse flags
# ---------------------------------------------------------------------------
if [[ "${1:-}" == "--help" ]] || [[ "${1:-}" == "-h" ]]; then
  cat <<'EOF'
check_secrets.sh — Scan the codebase for hardcoded secrets

Usage: bash scripts/check_secrets.sh [--help]

Patterns detected:
  - Hardcoded passwords in source files
  - Hardcoded API keys
  - AWS access key IDs (AKIA...)
  - AWS secret access keys
  - Private key PEM blocks
  - JWT secrets
  - Database URLs with embedded credentials
  - Hardcoded Bearer tokens
  - Long hex or base64 values labelled as secrets/keys/tokens
  - Committed .env files

False positives can be suppressed by adding a trailing comment:
  let x = "value"; // nocheck-secrets
EOF
  exit 0
fi

echo "=== SorobanPulse Secret Detection ==="
echo "Scanning: $ROOT_DIR"
echo ""

FAILED=0
WARNINGS=0

# ---------------------------------------------------------------------------
# Directories and files to skip
# ---------------------------------------------------------------------------
SKIP_DIRS=(
  "target"
  ".git"
  "node_modules"
  "sdk/python/.venv"
  "sdk/python/openapi_client"
  ".cargo"
)

EXCLUDE_ARGS=()
for d in "${SKIP_DIRS[@]}"; do
  EXCLUDE_ARGS+=("--exclude-dir=$d")
done

# Always exclude the lock file (very noisy, no secrets there)
EXCLUDE_ARGS+=("--exclude=Cargo.lock")
EXCLUDE_ARGS+=("--exclude=check_secrets.sh")
EXCLUDE_ARGS+=("--exclude=*.md")

# ---------------------------------------------------------------------------
# Helper: run a grep check and report findings
# ---------------------------------------------------------------------------
check_pattern() {
  local description="$1"
  local pattern="$2"
  local severity="${3:-WARN}"  # WARN or FAIL

  local matches
  # Use process substitution to avoid subshell exit-code issues with set -e
  if matches=$(grep -rn --include="*.rs" --include="*.toml" \
      --include="*.yml" --include="*.yaml" --include="*.env" \
      --include="*.json" --include="*.sh" --include="*.py" \
      --include="*.ts" --include="*.js" \
      "${EXCLUDE_ARGS[@]}" \
      -E "$pattern" "$ROOT_DIR" 2>/dev/null \
      | grep -v "nocheck-secrets" \
      | grep -v "//.*test\|#.*test\|_test\.\|test_\|mock_\|\.example\|placeholder\|CHANGEME\|YOUR_.*_HERE\|<.*>\|TODO\|FIXME" \
      || true); then
    if [[ -n "$matches" ]]; then
      echo "[$severity] $description:"
      echo "$matches" | head -10
      echo ""
      if [[ "$severity" == "FAIL" ]]; then
        FAILED=1
      else
        WARNINGS=$((WARNINGS + 1))
      fi
    fi
  fi
}

# ---------------------------------------------------------------------------
# Checks
# ---------------------------------------------------------------------------

# Hardcoded passwords (= "...") in non-test, non-example files
check_pattern \
  "Hardcoded password literals" \
  '(password|passwd|pwd)\s*[=:]\s*["'"'"'][^"'"'"']{4,}["'"'"']' \
  "FAIL"

# Hardcoded API keys
check_pattern \
  "Hardcoded API key literals" \
  '(api_key|apikey|api-key)\s*[=:]\s*["'"'"'][a-zA-Z0-9_\-]{16,}["'"'"']' \
  "FAIL"

# AWS access key IDs
check_pattern \
  "AWS Access Key ID pattern" \
  'AKIA[0-9A-Z]{16}' \
  "FAIL"

# AWS secret access key (not just the variable name)
check_pattern \
  "AWS Secret Access Key value" \
  '(aws_secret_access_key|AWS_SECRET_ACCESS_KEY)\s*[=:]\s*["'"'"'][^"'"'"']{20,}["'"'"']' \
  "FAIL"

# Private key PEM blocks
check_pattern \
  "Private key PEM block" \
  '-----BEGIN (RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----' \
  "FAIL"

# JWT secret literals
check_pattern \
  "JWT secret literal" \
  '(jwt_secret|JWT_SECRET)\s*[=:]\s*["'"'"'][^"'"'"']{8,}["'"'"']' \
  "FAIL"

# Database URLs with inline credentials (not from env vars)
check_pattern \
  "Database URL with embedded credentials" \
  'postgres(ql)?://[^$\{][^:]*:[^$\{@]{4,}@' \
  "WARN"

# Hardcoded Bearer tokens (long values, not env var references)
check_pattern \
  "Hardcoded Bearer token" \
  'Authorization:\s*Bearer [a-zA-Z0-9._\-]{20,}' \
  "WARN"

# Long hex strings labeled as secret/key/token
check_pattern \
  "Long hex value labeled as secret/key/token" \
  '(secret|private_key|token)\s*[=:]\s*["'"'"'][0-9a-fA-F]{64,}["'"'"']' \
  "WARN"

# Long base64 values labeled as secret/key/token
check_pattern \
  "Long base64 value labeled as secret/key/token" \
  '(secret|private_key|token)\s*[=:]\s*["'"'"'][A-Za-z0-9+/]{40,}={0,2}["'"'"']' \
  "WARN"

# GCP service account key markers
check_pattern \
  "GCP service account key" \
  '"type"\s*:\s*"service_account"' \
  "FAIL"

# Slack webhook URLs
check_pattern \
  "Hardcoded Slack webhook URL" \
  'hooks\.slack\.com/services/T[A-Z0-9]+/B[A-Z0-9]+/[a-zA-Z0-9]+' \
  "WARN"

# GitHub tokens
check_pattern \
  "GitHub token pattern" \
  'gh[pousr]_[A-Za-z0-9_]{36,}' \
  "FAIL"

# Generic high-entropy password patterns in env-style assignments
check_pattern \
  "High-entropy secret in env-style assignment (review manually)" \
  '^[A-Z_]+_SECRET\s*=\s*[^$\{][^[:space:]]{16,}' \
  "WARN"

# ---------------------------------------------------------------------------
# Check that .env is not tracked by git
# ---------------------------------------------------------------------------
if git -C "$ROOT_DIR" ls-files --error-unmatch .env 2>/dev/null; then
  echo "[FAIL] .env file is tracked by git!"
  echo "       Run: git rm --cached .env && echo '.env' >> .gitignore"
  echo ""
  FAILED=1
else
  echo "[OK] .env is not tracked by git"
fi

# ---------------------------------------------------------------------------
# Verify .gitignore covers .env files
# ---------------------------------------------------------------------------
if grep -qE '^\.(env)$|^\.env$' "$ROOT_DIR/.gitignore" 2>/dev/null; then
  echo "[OK] .env is in .gitignore"
else
  echo "[WARN] .env not explicitly listed in .gitignore"
  WARNINGS=$((WARNINGS + 1))
fi
echo ""

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
if [[ "$FAILED" -eq 0 ]] && [[ "$WARNINGS" -eq 0 ]]; then
  echo "[PASS] No secrets or issues detected."
  exit 0
elif [[ "$FAILED" -eq 0 ]]; then
  echo "[PASS with warnings] No definite secrets found, but $WARNINGS pattern(s) flagged for manual review."
  echo "       These may be false positives. Investigate before merging."
  exit 0
else
  echo "[FAIL] Potential secrets detected. Review the above matches."
  echo "       If these are false positives, add '# nocheck-secrets' to the end of the line."
  exit 1
fi
