#!/bin/sh
# Creates migrations/YYYYMMDDHHMMSS_description.sql (UTC timestamp), see #333.
# Usage: scripts/new-migration.sh short_description
set -eu
export LC_ALL=C
usage() {
    echo "usage: $0 short_description   (snake_case: a-z, 0-9 and _)" >&2
    exit 1
}
[ $# -eq 1 ] || usage
case "$1" in
    '' | [!a-z0-9]* | *[!a-z0-9_]*) usage ;;
esac
root=$(CDPATH='' cd -P -- "$(dirname -- "$0")/.." >/dev/null && pwd)
dir="$root/migrations"
ts=$(date -u +%Y%m%d%H%M%S)
for existing in "$dir/${ts}"_*.sql; do
    if [ -e "$existing" ]; then
        echo "$existing already uses timestamp $ts, try again in a second" >&2
        exit 1
    fi
done
file="$dir/${ts}_$1.sql"
# No SQL yet on purpose: build.rs refuses an empty migration, so it cannot be
# recorded as applied on a dev database before it is written.
printf -- '-- %s\n' "$1" >"$file"
echo "$file"
