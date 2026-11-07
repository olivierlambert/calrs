#!/bin/sh
# Creates an empty migration named YYYYMMDDHHMMSS_description.sql (UTC), see #333.
# Usage: scripts/new-migration.sh short_description
set -eu
if [ $# -ne 1 ] || ! printf '%s' "$1" | grep -Eq '^[a-z0-9_]+$'; then
    echo "usage: $0 short_description   (snake_case: a-z, 0-9 and _)" >&2
    exit 1
fi
dir="$(cd "$(dirname "$0")/.." && pwd)/migrations"
file="$dir/$(date -u +%Y%m%d%H%M%S)_$1.sql"
if [ -e "$file" ]; then
    echo "$file already exists, try again in a second" >&2
    exit 1
fi
printf -- '-- %s\n' "$1" >"$file"
echo "$file"
