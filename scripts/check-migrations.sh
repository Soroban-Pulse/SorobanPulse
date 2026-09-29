#!/bin/bash
# Check for duplicate migration timestamps

set -e

MIGRATIONS_DIR="migrations"
DUPLICATES=$(ls "$MIGRATIONS_DIR"/*.sql | grep -v '\.down\.sql$' | sed 's/.*\///' | sed 's/_.*$//' | sort | uniq -d)

if [ -n "$DUPLICATES" ]; then
    echo "ERROR: Found duplicate migration timestamps:"
    echo "$DUPLICATES"
    exit 1
fi

echo "✓ All migration timestamps are unique"

# Every up-migration must have a .down.sql pair (legacy 20250826 file exempt).
MISSING=""
for f in "$MIGRATIONS_DIR"/*.sql; do
    case "$f" in *.down.sql) continue ;; esac
    case "$f" in */20250826_*) continue ;; esac
    [ -f "${f%.sql}.down.sql" ] || MISSING="$MISSING $f"
done
if [ -n "$MISSING" ]; then
    echo "ERROR: Migrations missing a .down.sql pair:"
    for m in $MISSING; do echo "  $m"; done
    exit 1
fi
echo "✓ All migrations have down files"
