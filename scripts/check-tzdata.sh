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
# Fetched on its own: in a pipeline the status would be the filter's, and a
# failed download would read as an empty, "current" version.
if ! raw=$(curl -fsS --retry 3 --max-time 30 https://data.iana.org/time-zones/tzdb/version); then
    echo "cannot read the latest tz version from data.iana.org" >&2
    exit 2
fi
latest=$(printf '%s' "$raw" | tr -d '[:space:]')

# A version is a year and one or more letters (2026f). Anything else, empty
# included, means the check did not run.
valid() {
    case "$1" in
        [0-9][0-9][0-9][0-9][a-z]) return 0 ;;
        [0-9][0-9][0-9][0-9][a-z][a-z]) return 0 ;;
        *) return 1 ;;
    esac
}
if ! valid "$ours" || ! valid "$latest"; then
    echo "unexpected version strings: ours '$ours', latest '$latest'" >&2
    exit 2
fi

echo "ours=$ours"
echo "latest=$latest"

# Versions are a year and a letter (2026f), so they sort as strings.
if [ "$latest" \> "$ours" ]; then
    echo "tzdata $latest is out; calrs bundles $ours. Refresh vendor/chrono-tz/tz as vendor/chrono-tz/CALRS.md describes." >&2
    exit 1
fi
echo "tzdata $ours is current"
