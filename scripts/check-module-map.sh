#!/usr/bin/env bash
# scripts/check-module-map.sh
#
# CI guard: warn when a new top-level src/*.rs module is missing from
# docs/module-map.md.
#
# Exit code 0 = all modules accounted for.
# Exit code 1 = one or more modules are missing from the map.
#
# The script intentionally does NOT check sub-modules (src/middleware/,
# src/models/, etc.) because those are documented by their parent entry.
# It also skips the special files main.rs, lib.rs, and build.rs.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODULE_MAP="${REPO_ROOT}/docs/module-map.md"
SRC_DIR="${REPO_ROOT}/src"

if [[ ! -f "${MODULE_MAP}" ]]; then
  echo "ERROR: docs/module-map.md not found. Please create it before continuing." >&2
  exit 1
fi

# Collect every top-level *.rs file in src/ (not in subdirectories).
# Strip the .rs suffix and skip the well-known entry-point files.
SKIP_FILES=("main" "lib")

missing=()

while IFS= read -r file; do
  module_name="$(basename "${file}" .rs)"

  # Skip entry-point files that are never listed in the module map.
  skip=false
  for s in "${SKIP_FILES[@]}"; do
    if [[ "${module_name}" == "${s}" ]]; then
      skip=true
      break
    fi
  done
  [[ "${skip}" == "true" ]] && continue

  # Check whether the module name appears anywhere in the module map.
  if ! grep -q "${module_name}" "${MODULE_MAP}"; then
    missing+=("${module_name}")
  fi
done < <(find "${SRC_DIR}" -maxdepth 1 -name "*.rs" | sort)

if [[ ${#missing[@]} -eq 0 ]]; then
  echo "module-map check passed: all top-level src/ modules are documented."
  exit 0
fi

echo "" >&2
echo "================================================================" >&2
echo "  MODULE MAP CHECK FAILED" >&2
echo "================================================================" >&2
echo "" >&2
echo "The following top-level src/ modules are not mentioned in" >&2
echo "docs/module-map.md:" >&2
echo "" >&2
for m in "${missing[@]}"; do
  echo "  - ${m}.rs" >&2
done
echo "" >&2
echo "Add each missing module to the appropriate domain section in" >&2
echo "docs/module-map.md and commit the update alongside your code." >&2
echo "" >&2
exit 1
