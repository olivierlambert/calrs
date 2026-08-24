# Vendored: intl-tel-input

Upstream: <https://github.com/jackocnr/intl-tel-input>
Version: **25.3.1**

Vendored rather than loaded from a CDN so a booking page makes no third-party
request. calrs is self-hostable and air-gappable, and the captcha work already
established that a guest-facing page should not reach out to anyone. These
files are embedded in the binary with `include_str!` / `include_bytes!` and
served from `/static/intl-tel-input/` by `src/web/mod.rs`.

## Licensing

Two licences apply, because the bundle is two projects:

| File | Licence | Holder |
|---|---|---|
| `intlTelInput.min.js`, `intlTelInput.min.css`, `flags*.webp`, `globe*.webp` | MIT, see `LICENSE` | Jack O'Connor |
| `utils.js` | Apache-2.0 | The Closure Library Authors / Google (libphonenumber) |

`utils.js` is a Closure-compiled build of Google's libphonenumber. Its
Apache-2.0 SPDX header and copyright notice are retained verbatim at the top
of the file, which is where the licence requires them to stay. Do not strip
the header when updating.

## Updating

1. `npm pack intl-tel-input@<version>` and copy `build/js/intlTelInput.min.js`,
   `build/js/utils.js`, `build/css/intlTelInput.min.css`, and the four images
   from `build/img/`.
2. Keep the file names identical: `intlTelInput.min.css` references the images
   by flat relative URL, so all seven files must stay under one path prefix.
3. Check `utils.js` still ends with `export default utils;`. It is loaded as an
   ES module via dynamic `import()`, so a UMD build would break lazy loading.
4. Re-run `cargo test`, which asserts the assets are present, non-empty, and
   that `utils.js` keeps its Apache-2.0 notice.
