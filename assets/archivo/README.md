# Vendored: Archivo

Upstream: <https://github.com/Omnibus-Type/Archivo>, packaged by Fontsource as
`@fontsource-variable/archivo` 5.3.0 (the `wdth-normal` files: one variable
font with a weight axis, 100 to 900, and a width axis, 62 % to 125 %).

The dashboard's typeface. Vendored rather than loaded from a CDN so no page
makes a third-party request (the CSP is `default-src 'self'`). The two files
are embedded in the binary with `include_bytes!` and served from `/fonts/`
by `src/web/branding.rs`, like Inter for the guest pages.

| File | Subset |
|---|---|
| `archivo-latin.woff2` | Basic Latin and Latin-1 |
| `archivo-latin-ext.woff2` | Latin Extended (Polish, Estonian, ...) |

## Licence

SIL Open Font License 1.1, see `OFL.txt`. Copyright 2020 The Archivo Project
Authors. The licence travels with the font files; keep `OFL.txt` next to them
when updating.

## Updating

Download `files/archivo-latin-wdth-normal.woff2` and
`files/archivo-latin-ext-wdth-normal.woff2` from the npm package, replace the
two files, and keep the `unicode-range` values in `templates/dashboard_base.html`
in step with the package's `wdth.css`.
