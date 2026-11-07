# Vendored chrono-tz

This is chrono-tz 0.8.6 from crates.io, unchanged except for:

- `tz/`: the nine data files its build script reads (`africa`, `antarctica`,
  `asia`, `australasia`, `backward`, `etcetera`, `europe`, `northamerica`,
  `southamerica`) are IANA tzdata **2026b** in the *rearguard* form, with
  `NEWS` (the build reads the version from it), `LICENSE` and `version`. The
  files the build does not read are removed.
- `build.rs`: one `cargo:rerun-if-changed=tz` line. chrono-tz-build only
  reruns on an environment variable, so without it a refreshed `tz/` is
  silently ignored by an existing `target/`.
- `src/lib.rs`: one `#![allow(deprecated)]`. CI builds path dependencies with
  `-D warnings`, and 0.8.6 calls a chrono method deprecated since.

Rearguard, not the main form: since 2024b the main form writes numeric
abbreviations as `%z`, which this version's parser does not know and would
copy literally into every such abbreviation. Rearguard is IANA's form for
parsers lacking newer features. It also writes negative DST (Dublin,
Morocco, Namibia) as positive; the resulting UTC offsets are identical to the
main form's, checked every 6 hours from 1900 to 2040 for every zone.

Why: chrono-tz bundles its tz data at build time, and 0.8.6 carries 2024a.
tzdata 2026a/2026b move British Columbia (`America/Vancouver`) to permanent
-07 from 2026-11-01, which no chrono-tz release includes yet.

`IANA_TZDB_VERSION` reports `2026a`: the build reads it from the first line
of `NEWS`, and upstream's 2026b `NEWS` still heads its entry "Release 2026a".
`tz/version` holds the real one.

## Refreshing

1. Download `tzdata<version>.tar.gz` and its `.asc` from
   https://data.iana.org/time-zones/releases/ and check the signature against
   Paul Eggert's key, fingerprint `7E37 92A9 D8AC F7D6 33BC 1588 ED97 E90E 62AA 7E34`.
2. Convert each of the nine data files to rearguard form and copy them,
   with `NEWS`, `LICENSE` and `version`, into `tz/`:
   `awk -v DATAFORM=rearguard -f ziguard.awk europe > tz/europe` (and so on).
   Check `grep '%z' tz/*` only finds comments.
3. `cargo test` (the `tzdata` tests in `src/` pin the changes we rely on).

Remove `vendor/chrono-tz` and the `[patch.crates-io]` entry in `Cargo.toml`
once a chrono-tz release ships data at least this recent.
