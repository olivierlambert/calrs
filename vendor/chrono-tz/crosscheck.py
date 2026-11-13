#!/usr/bin/env python3
"""Expected UTC offsets at every transition, from IANA's own tools.

Usage: crosscheck.py <tzdata source dir> <out.tsv>

Compiles the main form of the nine data files with zic, then writes, for
every zone, each transition zdump reports from 1800 to 2040 (the second
before it and the second it starts), and the offset on the 15th of every
month at 12:00 UTC from 1900 to 2040 (read from the compiled file with
Python's zoneinfo), so a seasonal rule that should have ended shows up
between transitions too. One `zone<TAB>unix time<TAB>offset` per line, and
a last `#end<TAB>zones<TAB>lines` record the test requires, so a truncated
file fails.
`TZ_EXPECT=<out.tsv> cargo test tzdata_matches_zdump -- --ignored` then checks
the vendored chrono-tz against it. See CALRS.md.
"""
import datetime
import os
import shutil
import subprocess
import sys
import tempfile
import zoneinfo

FILES = ["africa", "antarctica", "asia", "australasia", "backward",
         "etcetera", "europe", "northamerica", "southamerica"]


def tool(name):
    # zic usually lives in /usr/sbin, which is not on a user's PATH.
    found = shutil.which(name) or shutil.which(name, path="/usr/sbin:/sbin")
    if not found:
        sys.exit(f"{name} not found (Debian/Ubuntu: apt install tzdata)")
    return found


def main(src, out_path):
    zic, zdump = tool("zic"), tool("zdump")
    with tempfile.TemporaryDirectory() as zdir:
        subprocess.run([zic, "-d", zdir] + FILES, cwd=src, check=True)
        zones = sorted(
            os.path.relpath(os.path.join(d, f), zdir)
            for d, _, fs in os.walk(zdir) for f in fs
        )
        n = 0
        with open(out_path, "w") as out:
            for zone in zones:
                # By name, read from the compiled tree through TZDIR (not the
                # system's zoneinfo): the output then starts with the zone
                # name, which has no spaces, whatever the temporary path.
                dump = subprocess.run(
                    [zdump, "-c", "1800,2041", "-v", zone],
                    capture_output=True, text=True, check=True,
                    env={"TZ": "", "TZDIR": zdir},
                ).stdout
                for line in dump.splitlines():
                    parts = line.split()
                    gmtoff = [p for p in parts if p.startswith("gmtoff=")]
                    if not gmtoff or "failed)" in line:
                        continue  # "NULL" lines, and zdump's bounds of time
                    if parts[0] != zone or parts[6] != "UT":
                        sys.exit(f"unexpected zdump line: {line}")
                    if not 1800 <= int(parts[5]) <= 2041:
                        continue  # zdump's bounds of time, outside the range
                    at = datetime.datetime.strptime(
                        " ".join(parts[2:6]), "%b %d %H:%M:%S %Y"
                    ).replace(tzinfo=datetime.timezone.utc)
                    out.write(f"{zone}\t{int(at.timestamp())}\t{gmtoff[0][7:]}\n")
                    n += 1
                with open(os.path.join(zdir, zone), "rb") as f:
                    tz = zoneinfo.ZoneInfo.from_file(f)
                for year in range(1900, 2041):
                    for month in range(1, 13):
                        at = datetime.datetime(year, month, 15, 12, tzinfo=datetime.timezone.utc)
                        offset = int(at.astimezone(tz).utcoffset().total_seconds())
                        out.write(f"{zone}\t{int(at.timestamp())}\t{offset}\n")
                        n += 1
            out.write(f"#end\t{len(zones)}\t{n}\n")
    print(f"{n} offsets in {len(zones)} zones")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
