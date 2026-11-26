#!/usr/bin/env bash
# Repository integrity checks.
#
# Runs cheap static checks that catch common breakages before merge:
#   1. Parse every tracked JSON / TOML / YAML file.
#   2. Grep for leftover merge conflict markers.
#   3. Validate migration versions (delegates to scripts/check-migrations.sh).
#   4. Run actionlint on GitHub workflows.
#   5. Detect orphaned Rust modules (delegates to scripts/check-module-map.sh).
#
# Exits non-zero with a clear message on any failure.

set -uo pipefail

# Resolve repo root (directory containing this script's parent).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}" || { echo "ERROR: cannot cd to repo root ${REPO_ROOT}" >&2; exit 1; }

FAILURES=0
fail() {
  echo "FAIL: $*" >&2
  FAILURES=$((FAILURES + 1))
}

# Collect tracked files (fall back to find if not a git checkout).
list_tracked() {
  if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    git ls-files
  else
    find . -type f -not -path './.git/*'
  fi
}

TRACKED_FILES="$(list_tracked)"

# ---------------------------------------------------------------------------
# 1. Parse JSON / TOML / YAML files.
# ---------------------------------------------------------------------------
echo "==> Checking JSON / TOML / YAML syntax"

have_jq=0
command -v jq >/dev/null 2>&1 && have_jq=1
have_py=0
command -v python3 >/dev/null 2>&1 && have_py=1

if [ "${have_jq}" -eq 0 ]; then
  fail "jq is required for JSON validation but was not found"
fi
if [ "${have_py}" -eq 0 ]; then
  fail "python3 is required for TOML/YAML validation but was not found"
fi

# Strip // and /* */ comments plus trailing commas from JSONC so jq can parse it.
strip_jsonc() {
  python3 - "$1" <<'PY'
import re
import sys

path = sys.argv[1]
with open(path, "r", encoding="utf-8") as fh:
    text = fh.read()

out = []
i = 0
n = len(text)
in_string = False
while i < n:
    ch = text[i]
    if in_string:
        out.append(ch)
        if ch == "\\" and i + 1 < n:
            out.append(text[i + 1])
            i += 2
            continue
        if ch == '"':
            in_string = False
        i += 1
        continue
    if ch == '"':
        in_string = True
        out.append(ch)
        i += 1
        continue
    if ch == "/" and i + 1 < n and text[i + 1] == "/":
        while i < n and text[i] != "\n":
            i += 1
        continue
    if ch == "/" and i + 1 < n and text[i + 1] == "*":
        i += 2
        while i + 1 < n and not (text[i] == "*" and text[i + 1] == "/"):
            i += 1
        i += 2
        continue
    out.append(ch)
    i += 1

result = "".join(out)
# Remove trailing commas before } or ].
result = re.sub(r",(\s*[}\]])", r"\1", result)
sys.stdout.write(result)
PY
}

is_jsonc() {
  case "$1" in
    tsconfig*.json|*/tsconfig*.json) return 0 ;;
    .devcontainer/*.json|*/.devcontainer/*.json) return 0 ;;
  esac
  return 1
}

while IFS= read -r file; do
  [ -z "${file}" ] && continue
  [ -f "${file}" ] || continue
  case "${file}" in
    *.json)
      if [ "${have_jq}" -eq 1 ]; then
        if is_jsonc "${file}"; then
          if ! strip_jsonc "${file}" | jq empty >/dev/null 2>&1; then
            fail "invalid JSONC: ${file}"
          fi
        else
          if ! jq empty "${file}" >/dev/null 2>&1; then
            fail "invalid JSON: ${file}"
          fi
        fi
      fi
      ;;
  esac
done <<< "${TRACKED_FILES}"

if [ "${have_py}" -eq 1 ]; then
  while IFS= read -r file; do
    [ -z "${file}" ] && continue
    [ -f "${file}" ] || continue
    case "${file}" in
      *.toml)
        if ! python3 -c 'import tomllib,sys; tomllib.load(open(sys.argv[1],"rb"))' "${file}" >/dev/null 2>&1; then
          fail "invalid TOML: ${file}"
        fi
        ;;
    esac
  done <<< "${TRACKED_FILES}"

  while IFS= read -r file; do
    [ -z "${file}" ] && continue
    [ -f "${file}" ] || continue
    case "${file}" in
      *.yml|*.yaml)
        # Exclude Helm templates (they contain Go templating, not valid YAML).
        case "${file}" in
          charts/*/templates/*|*/charts/*/templates/*) continue ;;
        esac
        if ! python3 -c 'import yaml,sys; yaml.safe_load(open(sys.argv[1]))' "${file}" >/dev/null 2>&1; then
          fail "invalid YAML: ${file}"
        fi
        ;;
    esac
  done <<< "${TRACKED_FILES}"
fi

# ---------------------------------------------------------------------------
# 2. Grep for conflict markers.
# ---------------------------------------------------------------------------
echo "==> Checking for merge conflict markers"

while IFS= read -r file; do
  [ -z "${file}" ] && continue
  [ -f "${file}" ] || continue
  # Skip binary files.
  if ! grep -Iq . "${file}" 2>/dev/null; then
    continue
  fi
  if grep -nE '^(<<<<<<< |=======$|>>>>>>> )' "${file}" >/dev/null 2>&1; then
    fail "conflict markers found in: ${file}"
    grep -nE '^(<<<<<<< |=======$|>>>>>>> )' "${file}" | sed 's/^/    /' >&2
  fi
done <<< "${TRACKED_FILES}"

# ---------------------------------------------------------------------------
# 3. Validate migrations.
# ---------------------------------------------------------------------------
echo "==> Checking migrations"
if [ -x "${SCRIPT_DIR}/check-migrations.sh" ]; then
  if ! "${SCRIPT_DIR}/check-migrations.sh"; then
    fail "migration check failed (scripts/check-migrations.sh)"
  fi
elif [ -f "${SCRIPT_DIR}/check-migrations.sh" ]; then
  if ! bash "${SCRIPT_DIR}/check-migrations.sh"; then
    fail "migration check failed (scripts/check-migrations.sh)"
  fi
else
  echo "    (scripts/check-migrations.sh not found, skipping)"
fi

# ---------------------------------------------------------------------------
# 4. Run actionlint on workflows.
# ---------------------------------------------------------------------------
echo "==> Running actionlint"
if command -v actionlint >/dev/null 2>&1; then
  shopt -s nullglob
  workflows=(.github/workflows/*.yml .github/workflows/*.yaml)
  shopt -u nullglob
  if [ "${#workflows[@]}" -gt 0 ]; then
    if ! actionlint "${workflows[@]}"; then
      fail "actionlint reported problems"
    fi
  fi
else
  echo "    (actionlint not installed, skipping)"
fi

# ---------------------------------------------------------------------------
# 5. Detect orphaned Rust modules.
# ---------------------------------------------------------------------------
echo "==> Checking Rust module map"
if [ -x "${SCRIPT_DIR}/check-module-map.sh" ]; then
  if ! "${SCRIPT_DIR}/check-module-map.sh"; then
    fail "module map check failed (scripts/check-module-map.sh)"
  fi
elif [ -f "${SCRIPT_DIR}/check-module-map.sh" ]; then
  if ! bash "${SCRIPT_DIR}/check-module-map.sh"; then
    fail "module map check failed (scripts/check-module-map.sh)"
  fi
else
  echo "    (scripts/check-module-map.sh not found, skipping)"
fi

# ---------------------------------------------------------------------------
# Summary.
# ---------------------------------------------------------------------------
echo
if [ "${FAILURES}" -ne 0 ]; then
  echo "Repository integrity check FAILED with ${FAILURES} problem(s)." >&2
  exit 1
fi

echo "Repository integrity check passed."
exit 0