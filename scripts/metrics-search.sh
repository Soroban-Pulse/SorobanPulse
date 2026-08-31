#!/bin/bash
# Search Soroban Pulse's Prometheus metrics by substring: source definitions
# plus the matching row in docs/metrics-reference.md, if one exists.
#
# Usage: ./scripts/metrics-search.sh <substring>
# Example: ./scripts/metrics-search.sh lag

set -euo pipefail

if [ $# -ne 1 ] || [ -z "$1" ]; then
    echo "Usage: $0 <substring>" >&2
    echo "Example: $0 lag" >&2
    exit 1
fi

QUERY="$1"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REFERENCE_DOC="$ROOT/docs/metrics-reference.md"

echo "== Source definitions matching '$QUERY' =="
MATCHES=$(grep -rnoE "(counter|gauge|histogram)!\(\s*\"[a-z0-9_]*${QUERY}[a-z0-9_]*\"" \
    "$ROOT/src" --include='*.rs' 2>/dev/null || true)

if [ -z "$MATCHES" ]; then
    echo "(no metric names contain '$QUERY' under src/)"
else
    echo "$MATCHES" | sed -E 's/:([a-z]+)!\(\s*"/  ->  \1  /' | sed -E 's/"$//'
fi

echo
echo "== docs/metrics-reference.md rows mentioning '$QUERY' =="
if [ -f "$REFERENCE_DOC" ]; then
    grep -i "$QUERY" "$REFERENCE_DOC" | grep '^| `soroban_pulse_' || echo "(no reference rows mention '$QUERY')"
else
    echo "(docs/metrics-reference.md not found)"
fi
