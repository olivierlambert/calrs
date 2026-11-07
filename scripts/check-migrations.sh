#!/bin/sh
# Checks the migrations a branch adds since it left <base-revision> (#333):
# - every migration added must sort after the newest one on the base, so
#   fresh installs and upgraded databases apply them in the same order;
# - no migration of the base may be deleted or renamed, since deployed
#   databases record the name;
# - editing one only warns: deployed databases never re-run it.
# Usage: scripts/check-migrations.sh <base-revision>   (e.g. origin/main)
set -eu
export LC_ALL=C
[ $# -eq 1 ] || { echo "usage: $0 <base-revision>" >&2; exit 2; }
base=$1
newest=$(git ls-tree --name-only "$base" migrations/ | sed -n 's|^migrations/\(.*\)\.sql$|\1|p' | sort | tail -n 1)
fail=0
changes=$(git diff --no-renames --name-status "$base"...HEAD -- migrations/)
[ -n "$changes" ] || exit 0
printf '%s\n' "$changes" | {
    while IFS="$(printf '\t')" read -r status path; do
        name=${path#migrations/}
        name=${name%.sql}
        case "$status" in
            A)
                latest=$(printf '%s\n%s\n' "$newest" "$name" | sort | tail -n 1)
                if [ -n "$newest" ] && { [ "$name" = "$newest" ] || [ "$latest" != "$name" ]; }; then
                    echo "error: $path is older than $newest.sql, the newest migration on $base." >&2
                    echo "  Fresh installs and upgraded databases would apply them in different orders." >&2
                    echo "  Rename it with a fresh timestamp from scripts/new-migration.sh (git mv)." >&2
                    fail=1
                fi
                ;;
            D)
                echo "error: $path exists on $base and must stay: deployed databases record its name." >&2
                fail=1
                ;;
            M)
                echo "warning: $path changed; deployed databases already applied it and will not re-run it." >&2
                ;;
        esac
    done
    exit $fail
}
