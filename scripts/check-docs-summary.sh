#!/usr/bin/env bash
#
# check-docs-summary.sh
#
# Verifies that every Markdown file under docs/ (excluding docs/adr/,
# docs/archive/ and docs/design/) is reachable from docs/SUMMARY.md.
#
# mdBook only renders pages listed in SUMMARY.md, so any docs/*.md file that
# is not linked from it is effectively invisible on the published site.
#
# Usage:
#   scripts/check-docs-summary.sh [docs_dir]
#
# Exit codes:
#   0 - all docs are linked from SUMMARY.md
#   1 - one or more orphan pages were found (or a required file is missing)
#
set -euo pipefail

DOCS_DIR="${1:-docs}"
SUMMARY_FILE="${DOCS_DIR}/SUMMARY.md"

if [[ ! -d "${DOCS_DIR}" ]]; then
    echo "error: docs directory '${DOCS_DIR}' not found" >&2
    exit 1
fi

if [[ ! -f "${SUMMARY_FILE}" ]]; then
    echo "error: summary file '${SUMMARY_FILE}' not found" >&2
    exit 1
fi

# Directories whose contents are exempt from the SUMMARY.md requirement.
EXCLUDED_DIRS=("adr" "archive" "design")

# Build a find expression that prunes the excluded directories.
prune_expr=()
for dir in "${EXCLUDED_DIRS[@]}"; do
    prune_expr+=(-path "${DOCS_DIR}/${dir}" -prune -o)
done

# Collect all candidate Markdown files, normalised to paths relative to the
# repository root (e.g. "docs/ML_INTEGRATION.md").
mapfile -t all_docs < <(
    find "${DOCS_DIR}" \
        "${prune_expr[@]}" \
        -type f -name '*.md' -print \
        | sed 's|^\./||' \
        | sort
)

# Extract link targets from SUMMARY.md. mdBook summary entries look like:
#   - [Title](path/to/page.md)
#   - [Title](path/to/page.md#anchor)
# We capture everything inside the parentheses and strip any anchor/query.
mapfile -t linked_docs < <(
    grep -oE '\]\([^)]+\)' "${SUMMARY_FILE}" \
        | sed -E 's/^\]\(//; s/\)$//' \
        | sed -E 's/[?#].*$//' \
        | sed 's|^\./||' \
        | sort -u
)

# Normalise SUMMARY.md links to be relative to the repository root. Links in
# SUMMARY.md are relative to the docs/ directory, so prefix them with the docs
# directory unless they already include it.
normalised_links=()
for link in "${linked_docs[@]}"; do
    [[ -z "${link}" ]] && continue
    # Skip external links and anchors.
    case "${link}" in
        http://*|https://*|mailto:*|\#*) continue ;;
    esac
    if [[ "${link}" == "${DOCS_DIR}/"* ]]; then
        normalised_links+=("${link}")
    else
        normalised_links+=("${DOCS_DIR}/${link}")
    fi
done

# Sort the normalised links for lookup.
mapfile -t sorted_links < <(printf '%s\n' "${normalised_links[@]}" | sort -u)

# Find orphan pages: docs present on disk but not referenced in SUMMARY.md.
orphans=()
for doc in "${all_docs[@]}"; do
    # SUMMARY.md itself is the index and is never listed within itself.
    if [[ "${doc}" == "${SUMMARY_FILE}" ]]; then
        continue
    fi
    found=0
    for link in "${sorted_links[@]}"; do
        if [[ "${doc}" == "${link}" ]]; then
            found=1
            break
        fi
    done
    if [[ "${found}" -eq 0 ]]; then
        orphans+=("${doc}")
    fi
done

if [[ "${#orphans[@]}" -gt 0 ]]; then
    echo "error: ${#orphans[@]} documentation page(s) are not linked from ${SUMMARY_FILE}:" >&2
    echo "" >&2
    for orphan in "${orphans[@]}"; do
        echo "  - ${orphan}" >&2
    done
    echo "" >&2
    echo "Every Markdown file under ${DOCS_DIR}/ (except ${DOCS_DIR}/adr/, ${DOCS_DIR}/archive/ and ${DOCS_DIR}/design/) must be listed in ${SUMMARY_FILE}." >&2
    echo "Add the page to ${SUMMARY_FILE}, or move it under ${DOCS_DIR}/adr/, ${DOCS_DIR}/archive/ or ${DOCS_DIR}/design/ if it is a design/planning or obsolete document." >&2
    exit 1
fi

echo "ok: all ${#all_docs[@]} documentation page(s) are linked from ${SUMMARY_FILE}"
exit 0