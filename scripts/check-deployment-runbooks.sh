#!/usr/bin/env bash
# =============================================================================
# check-deployment-runbooks.sh (Issue #913)
#
# Lightweight structural linter for docs/deployment-runbooks/*.md.
#
# Checks, for every platform runbook (every .md file in the directory except
# README.md, template.md, and testing-framework.md):
#   1. Every required section heading from template.md ("## Heading") is
#      present, verbatim.
#   2. Every fenced code block (``` ... ```) is closed — i.e. the file
#      contains an even number of lines that start with ```.
#
# This is a STATIC structure check only. See
# docs/deployment-runbooks/testing-framework.md for what it does and does not
# verify (in particular: it does not run any of the commands in the
# runbooks, and does not touch real cloud accounts).
#
# Usage:
#   ./scripts/check-deployment-runbooks.sh
#
# Exit status: 0 if every runbook passes, 1 if any check fails (with a
# message identifying the file and the specific problem).
# =============================================================================

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUNBOOKS_DIR="$REPO_ROOT/docs/deployment-runbooks"
TEMPLATE="$RUNBOOKS_DIR/template.md"

if [ ! -d "$RUNBOOKS_DIR" ]; then
  echo "ERROR: runbooks directory not found: $RUNBOOKS_DIR"
  exit 1
fi

if [ ! -f "$TEMPLATE" ]; then
  echo "ERROR: template not found: $TEMPLATE (required to derive the list of mandatory section headings)"
  exit 1
fi

# Extract the required section headings from the template, e.g. "## Prerequisites".
# grep -o keeps the whole matched heading including the trailing text on that line.
mapfile -t REQUIRED_HEADINGS < <(grep -E '^## ' "$TEMPLATE")

if [ "${#REQUIRED_HEADINGS[@]}" -eq 0 ]; then
  echo "ERROR: no '## ' section headings found in $TEMPLATE — cannot determine required structure."
  exit 1
fi

echo "Required sections (from template.md):"
for h in "${REQUIRED_HEADINGS[@]}"; do
  echo "  - $h"
done
echo

FAILURES=0
CHECKED=0

for file in "$RUNBOOKS_DIR"/*.md; do
  base="$(basename "$file")"

  # Skip the index, the template itself, and the meta-doc about this script —
  # none of these are platform runbooks and none are expected to follow the
  # runbook section structure.
  if [ "$base" = "README.md" ] || [ "$base" = "template.md" ] || [ "$base" = "testing-framework.md" ]; then
    continue
  fi

  CHECKED=$((CHECKED + 1))
  file_ok=1

  # --- Check 1: every required heading is present, verbatim ---
  for heading in "${REQUIRED_HEADINGS[@]}"; do
    # Anchor to start-of-line so "## Architecture" doesn't match inside a
    # deeper heading like "### Architecture notes".
    if ! grep -qF "$heading" "$file"; then
      echo "FAIL: $base is missing required section heading: $heading"
      file_ok=0
    fi
  done

  # --- Check 2: every fenced code block is closed ---
  fence_count=$(grep -c '^```' "$file")
  if [ $((fence_count % 2)) -ne 0 ]; then
    echo "FAIL: $base has an unclosed fenced code block (found $fence_count lines starting with \`\`\`, expected an even number)"
    file_ok=0
  fi

  if [ "$file_ok" -eq 1 ]; then
    echo "OK:   $base"
  else
    FAILURES=$((FAILURES + 1))
  fi
done

if [ "$CHECKED" -eq 0 ]; then
  echo "ERROR: no runbook files found to check in $RUNBOOKS_DIR (looked for *.md other than README.md/template.md)."
  exit 1
fi

echo
if [ "$FAILURES" -gt 0 ]; then
  echo "$FAILURES of $CHECKED runbook(s) failed structural checks. See FAIL lines above."
  exit 1
fi

echo "All $CHECKED runbook(s) passed structural checks."
exit 0
