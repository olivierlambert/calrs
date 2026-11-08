# Vendored: Swagger UI

Upstream: <https://github.com/swagger-api/swagger-ui> (npm package
`swagger-ui-dist`)
Version: **5.34.1**

The interactive reference of the REST API at `/api/docs` (#169) renders the
OpenAPI document calrs generates (`/api/v1/openapi.json`) with Swagger UI.
The files are vendored rather than loaded from a CDN, so the page makes no
third-party request (calrs is self-hostable and air-gappable), and no build
step downloads anything. They are embedded in the binary with
`include_bytes!` and served from `/static/swagger-ui/` by
`src/web/api_doc.rs`.

Only two files of the package are used: `swagger-ui-bundle.js` (the UI and
its dependencies, about 1.7 MB) and `swagger-ui.css` (about 190 KB). The
standalone preset, the top bar and the OAuth redirect page are not needed:
calrs passes the document's URL itself.

Why Swagger UI: it is the reference viewer with a working "Try it out",
renders OpenAPI 3.1, has a dark mode, needs neither `eval` nor any external
font or request, and has a long, steady release history. RapiDoc is smaller
(about 640 KB, MIT) but shipped three major versions in the five weeks before
this was vendored; Redoc has no "Try it out"; Scalar's standalone bundle is
about 4.9 MB, and loads its fonts from a CDN by default.

## Licensing

| File | Licence | Holder |
|---|---|---|
| `swagger-ui-bundle.js`, `swagger-ui.css` | Apache-2.0, see `LICENSE` and `NOTICE` | SmartBear Software Inc. |
| Third-party code inside `swagger-ui-bundle.js` | MIT, BSD-3-Clause, and Apache-2.0 or MPL-2.0 (DOMPurify), see `swagger-ui-bundle.js.LICENSE.txt` | Their authors |

Apache-2.0 is compatible with calrs's AGPL-3.0. Its `NOTICE` must travel
with the files, and `swagger-ui-bundle.js` starts with a pointer to
`swagger-ui-bundle.js.LICENSE.txt`, which collects the notices of the code
bundled into it. Keep all three files next to the bundle.

## Integrity

The files are byte-identical to the npm package. SHA-256:

```
36eeb4d5640b9c036b1d0f854499af1cceebbdb260b790a03a18be5e8488fbf5  swagger-ui-bundle.js
c3cfe8248c4b0b89673600fb829924eafbac6de4bc4b2b06f5f5243483074d0a  swagger-ui.css
```

`src/web/tests/api_doc.rs` pins these hashes, so a changed file fails the
tests until the change is reviewed and the hashes updated here and there.

## Updating

1. `npm pack swagger-ui-dist@<version>` in an empty directory and extract it.
2. Copy `swagger-ui-bundle.js`, `swagger-ui.css`, `LICENSE`, `NOTICE` and
   `swagger-ui-bundle.js.LICENSE.txt` from `package/` here, keeping the names.
3. Check the bundle still makes no request of its own: `grep -o
   'https\?://[a-z0-9./_-]*' swagger-ui-bundle.js | sort -u` should list only
   specification and licence links, plus `validator.swagger.io`, which the
   page turns off (`validatorUrl: null`).
4. Update the version here and `SWAGGER_UI_VERSION` in `src/web/api_doc.rs`
   (it busts the browser cache of the old files), and the hashes here and in
   the test.
5. Run `cargo test api_doc`, then open `/api/docs` with the browser console
   open: no Content-Security-Policy violation, no request to another origin.
