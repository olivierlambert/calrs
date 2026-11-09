# Phosphor Icons (vendored subset)

The dashboard's icons: the sidebar navigation, and a few places where an icon
carries meaning (Join, copy a link, the set-up line, the next meeting's time
and place).

- Source: https://github.com/phosphor-icons/core, npm package
  `@phosphor-icons/core`, files `assets/regular/<name>.svg`
- Version: **2.1.1**
- Weight: regular
- Licence: MIT, see `LICENSE` (copied from the package)

Only the icons calrs uses are kept, unchanged, in `regular/`. They are not
served as files: `gen_partial.py` copies their markup into the template
macro `templates/partials/phosphor.html`, so pages carry them inline and
make no extra request. To add one, download `regular/<name>.svg` from the
same version, then run `python3 assets/phosphor/gen_partial.py` from the
repository root. `vendored_phosphor_icons_keep_their_licence` (in
`src/web/tests/guards.rs`) fails if the licence goes missing or the
template drifts from the SVGs.
