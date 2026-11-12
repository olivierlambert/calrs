# Vendored chrono-tz

This is chrono-tz 0.8.6 from crates.io, unchanged except for:

- `tz/`: the nine data files its build script reads (`africa`, `antarctica`,
  `asia`, `australasia`, `backward`, `etcetera`, `europe`, `northamerica`,
  `southamerica`) are IANA tzdata **2026f** in the *rearguard* form, with
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
2026 moved several zones to permanent offsets, which no chrono-tz release
includes yet: British Columbia -07 (2026a/b), Alberta -06 and Morocco +00
(2026c), the Northwest Territories -06 (2026d), Manitoba -05 (2026e), and
Cuba staying on -04 until 2027-11-07 (2026f). `tzdata_*` tests in
`src/utils.rs` pin them.

The build reads `IANA_TZDB_VERSION` from the first `Release` line of `NEWS`;
`tz/version` holds the same version.

## Staying current

`scripts/check-tzdata.sh` compares `tz/version` with IANA's latest. CI runs
it on every push and pull request (job "tzdata current"), and the daily
`tzdata` workflow (on `main`) opens an issue when IANA publishes a new release, so a
stale copy is noticed before a zone's clocks change rather than after.

## Refreshing

1. Download `tzdata<version>.tar.gz` and its `.asc` from
   https://data.iana.org/time-zones/releases/ and check the signature against
   Paul Eggert's key, fingerprint `7E37 92A9 D8AC F7D6 33BC 1588 ED97 E90E 62AA 7E34`.
   If gpg reports the key expired, refresh it (`gpg --keyserver
   hkps://keys.openpgp.org --refresh-keys <fingerprint>`): some keyservers
   hold a copy that predates its extension.
2. Convert each of the nine data files to rearguard form and copy them,
   with `NEWS`, `LICENSE` and `version`, into `tz/`:
   `awk -v DATAFORM=rearguard -f ziguard.awk europe > tz/europe` (and so on).
   Check `grep '%z' tz/*` only finds comments.
3. Compare UTC offsets with the main form: compile both with `zic -d`, then
   compare, for every zone, the offset changes `zdump -c 1900,2041 -v`
   prints (only the negative-DST zones differ in their DST flag).
4. Add a `tzdata_*` test in `src/utils.rs` for any new rule change, and run
   `cargo test` (the tests pin the changes we rely on).
5. Apply the same refresh to the maintenance branch (`1.23.x`) and release
   both.

Remove `vendor/chrono-tz` and the `[patch.crates-io]` entry in `Cargo.toml`
once a chrono-tz release ships data at least this recent.
