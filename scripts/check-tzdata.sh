#!/bin/sh
# Fails when IANA has published a tz database newer than the one calrs
# bundles in vendor/chrono-tz/tz (#285). calrs converts every time through
# that copy, so a stale one puts zones that changed their rules an hour off.
#
# Exit status: 0 current, 1 stale (refresh per vendor/chrono-tz/CALRS.md),
# 2 the check itself could not run. Prints `ours=` and `latest=` lines for
# the scheduled workflow.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
ours=$(tr -d '[:space:]' < "$root/vendor/chrono-tz/tz/version")
latest=$(curl -fsS --retry 3 --max-time 30 https://data.iana.org/time-zones/tzdb/version | tr -d '[:space:]') || {
    echo "cannot read the latest tz version from data.iana.org" >&2
    exit 2
}

case "$ours$latest" in
    *[!0-9a-z]*) echo "unexpected version strings: '$ours' '$latest'" >&2; exit 2 ;;
esac

echo "ours=$ours"
echo "latest=$latest"

# Versions are a year and a letter (2026f), so they sort as strings.
if [ "$latest" \> "$ours" ]; then
    echo "tzdata $latest is out; calrs bundles $ours. Refresh vendor/chrono-tz/tz as vendor/chrono-tz/CALRS.md describes." >&2
    exit 1
fi
echo "tzdata $ours is current"
