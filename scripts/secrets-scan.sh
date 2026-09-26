#!/usr/bin/env bash
# scripts/secrets-scan.sh
#
# Local wrapper for secrets scanning using gitleaks.
# Usage:
#   ./scripts/secrets-scan.sh              # Scan working tree
#   ./scripts/secrets-scan.sh --history    # Scan last 50 commits
#   ./scripts/secrets-scan.sh --all        # Scan full git history

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")")" && echo "${SCRIPT_DIR}" > /dev/null
REPO_ROOT="$(git -C "${SCRIPT_DIR}" rev-parse --show-toplevel)"
CONFIG="${REPO_ROOT}/.gitleaks.toml"
MODE="${1:-}"

green()  { printf '\033[0;32m%s\033[0m\n' "$*"; }
red()    { printf '\033[0;31m%s\033[0m\n' "$*"; }
yellow() { printf '\033[0;33m%s\033[0m\n' "$*"; }

# ─── Gitleaks availability check ─────────────────────────────────────────────
if ! command -v gitleaks &>/dev/null; then
  yellow "gitleaks not found. Install instructions:"
  echo "  macOS:   brew install gitleaks"
  echo "  Linux:   https://github.com/gitleaks/gitleaks/releases"
  echo "  Docker:  docker run -v \"$(pwd):/repo\" ghcr.io/gitleaks/gitleaks:latest detect --source /repo"
  exit 1
fi

GITLEAKS_VERSION=$(gitleaks version 2>/dev/null || echo 'unknown')
echo "Running gitleaks ${GITLEAKS_VERSION}"
echo "Repository: ${REPO_ROOT}"
echo ""

# ─── Scan modes ───────────────────────────────────────────────────────────────
case "${MODE}" in
  --history)
    yellow "Scanning last 50 commits for secrets..."
    gitleaks detect \
      --source "${REPO_ROOT}" \
      --config "${CONFIG}" \
      --log-opts="HEAD~50..HEAD" \
      --verbose
    ;;
  --all)
    yellow "Scanning full git history for secrets (this may take a while)..."
    gitleaks detect \
      --source "${REPO_ROOT}" \
      --config "${CONFIG}" \
      --log-opts="--all" \
      --verbose
    ;;
  "")
    yellow "Scanning working tree for secrets..."
    gitleaks detect \
      --source "${REPO_ROOT}" \
      --config "${CONFIG}" \
      --no-git \
      --verbose
    ;;
  *)
    red "Unknown option: ${MODE}"
    echo "Usage: $0 [--history|--all]"
    exit 1
    ;;
esac

EXIT_CODE=$?
if [ "${EXIT_CODE}" -eq 0 ]; then
  green "\n✓ No secrets detected."
else
  red "\n✗ Secrets detected! Review the output above and remediate before committing."
  red "  See docs/secret-management.md for remediation guidance."
fi

exit "${EXIT_CODE}"
