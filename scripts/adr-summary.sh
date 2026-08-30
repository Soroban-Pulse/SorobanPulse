#!/bin/bash
# Print a Markdown status table for every ADR under docs/adr/, sorted by
# ADR number. Extracts the title from the first "# " heading and the
# Status/Date fields from the "- **Status:** ..." / "- **Date:** ..." lines.
#
# Usage: scripts/adr-summary.sh [adr-dir]
#   adr-dir defaults to docs/adr relative to the repo root.

set -euo pipefail

ADR_DIR="${1:-docs/adr}"

if [ ! -d "$ADR_DIR" ]; then
    echo "ERROR: ADR directory not found: $ADR_DIR" >&2
    exit 1
fi

echo "| ADR | Title | Status | Date |"
echo "|---|---|---|---|"

# Only real ADR records match NNNN-*.md; the template (0000-template.md) is
# excluded since it is not an accepted decision.
for f in "$ADR_DIR"/[0-9][0-9][0-9][0-9]-*.md; do
    [ -e "$f" ] || continue
    base="$(basename "$f")"
    case "$base" in
        0000-template.md) continue ;;
    esac

    number="${base%%-*}"

    title="$(grep -m1 '^# ' "$f" | sed -E 's/^#[[:space:]]*//')"
    status="$(grep -m1 -E '^\-[[:space:]]*\*\*Status:\*\*' "$f" | sed -E 's/^-[[:space:]]*\*\*Status:\*\*[[:space:]]*//')"
    date="$(grep -m1 -E '^\-[[:space:]]*\*\*Date:\*\*' "$f" | sed -E 's/^-[[:space:]]*\*\*Date:\*\*[[:space:]]*//')"

    echo "| ${number} | ${title:-unknown} | ${status:-unknown} | ${date:-unknown} |"
done | sort -t'|' -k2,2n
