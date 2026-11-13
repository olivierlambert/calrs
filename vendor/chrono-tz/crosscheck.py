#!/usr/bin/env python3
"""Expected UTC offsets at every transition, from IANA's own tools.

Usage: crosscheck.py <tzdata source dir> <out.tsv>

Compiles the main form of the nine data files with zic, then writes, for
every zone, each transition zdump reports from 1800 to 2040 (the second
before it and the second it starts) as `zone<TAB>unix time<TAB>offset`.
`TZ_EXPECT=<out.tsv> cargo test tzdata_matches_zdump -- --ignored` then checks
the vendored chrono-tz against it. See CALRS.md.
"""
import datetime
import os
import shutil
import subprocess
import sys
import tempfile

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
                dump = subprocess.run(
                    [zdump, "-c", "1800,2041", "-v", os.path.join(zdir, zone)],
                    capture_output=True, text=True, env={"TZ": ""}, check=True,
                ).stdout
                for line in dump.splitlines():
                    parts = line.split()
                    gmtoff = [p for p in parts if p.startswith("gmtoff=")]
                    if "UT" not in parts or not gmtoff:
                        continue
                    try:
                        at = datetime.datetime.strptime(
                            " ".join(parts[2:6]), "%b %d %H:%M:%S %Y"
                        ).replace(tzinfo=datetime.timezone.utc)
                    except ValueError:
                        continue
                    out.write(f"{zone}\t{int(at.timestamp())}\t{gmtoff[0][7:]}\n")
                    n += 1
    print(f"{n} transitions in {len(zones)} zones")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
