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

# ---------------------------------------------------------------------------
# Orphaned-module check
#
# Rust only compiles modules that are declared with `mod <name>;` (or
# `pub mod <name>;`) somewhere reachable from an entry point. A .rs file that
# is never declared is dead code: it is not type-checked, not tested, and not
# reachable at runtime.
#
# We consider a module reachable if its name is declared via `mod <name>;`
# in any of:
#   - src/lib.rs
#   - src/main.rs
#   - any file under src/bin/
#   - any other .rs file under src/ (transitive declarations, e.g. a
#     sub-module declared from a parent module file)
#
# Entry points themselves (lib.rs, main.rs, files under src/bin/) are roots
# and are never reported as orphans.
# ---------------------------------------------------------------------------

orphaned=()

# Build the set of all declared module names across the crate. We grep for
# `mod <name>;` declarations, allowing leading whitespace and an optional
# `pub` / `pub(crate)` / `pub(super)` / `pub(in ...)` visibility prefix.
declared_modules="$(
  {
    # Entry points first.
    [[ -f "${SRC_DIR}/lib.rs" ]] && cat "${SRC_DIR}/lib.rs"
    [[ -f "${SRC_DIR}/main.rs" ]] && cat "${SRC_DIR}/main.rs"
    # Any other .rs file under src/ (including src/bin/).
    find "${SRC_DIR}" -name "*.rs" \
      ! -path "${SRC_DIR}/lib.rs" \
      ! -path "${SRC_DIR}/main.rs" \
      -print0 2>/dev/null | xargs -0 -r cat 2>/dev/null || true
  } \
  | grep -oE '(^|[[:space:]])mod[[:space:]]+[A-Za-z_][A-Za-z0-9_]*[[:space:]]*;' \
  | sed -E 's/.*mod[[:space:]]+([A-Za-z_][A-Za-z0-9_]*)[[:space:]]*;/\1/' \
  | sort -u
)"

# Every .rs file under src/ (recursively) is a candidate module. We map the
# file path to the module name Rust would use for it:
#   src/foo.rs          -> foo
#   src/foo/mod.rs      -> foo
#   src/foo/bar.rs      -> bar
#   src/bin/thing.rs    -> thing (entry point, skipped)
#   src/bin/thing/main.rs -> entry point, skipped
while IFS= read -r file; do
  rel="${file#"${SRC_DIR}/"}"

  # Entry points are roots, never orphans.
  case "${rel}" in
    lib.rs|main.rs) continue ;;
    bin/*) continue ;;
  esac

  base="$(basename "${rel}")"
  if [[ "${base}" == "mod.rs" ]]; then
    module_name="$(basename "$(dirname "${rel}")")"
  else
    module_name="${base%.rs}"
  fi

  # A module is reachable if it is declared somewhere in the crate.
  if ! grep -qxF "${module_name}" <<<"${declared_modules}"; then
    orphaned+=("${rel}")
  fi
done < <(find "${SRC_DIR}" -name "*.rs" | sort)

status=0

if [[ ${#missing[@]} -ne 0 ]]; then
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
  status=1
fi

if [[ ${#orphaned[@]} -ne 0 ]]; then
  echo "" >&2
  echo "================================================================" >&2
  echo "  ORPHANED MODULE CHECK FAILED" >&2
  echo "================================================================" >&2
  echo "" >&2
  echo "The following .rs files under src/ are not reachable from any" >&2
  echo "entry point (src/lib.rs, src/main.rs, or src/bin/):" >&2
  echo "" >&2
  for f in "${orphaned[@]}"; do
    echo "  - src/${f}" >&2
  done
  echo "" >&2
  echo "Rust only compiles modules that are declared with \`mod <name>;\`." >&2
  echo "Either declare each module from a reachable parent (e.g. add" >&2
  echo "\`pub mod <name>;\` to src/lib.rs) or delete the file." >&2
  echo "" >&2
  status=1
fi

if [[ ${status} -eq 0 ]]; then
  echo "module-map check passed: all top-level src/ modules are documented and no orphaned modules were found."
  exit 0
fi

exit 1
