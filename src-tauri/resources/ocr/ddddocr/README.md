# Shared ddddocr model

All features use `crate::ocr::recognize`; the model and engine are not owned by
OJ. This directory contains only the pinned model manifest and its MIT license.

The default build does not embed the model. The app checks the verified cache at
startup and downloads the pinned `common.onnx` on demand when a captcha feature
is first used. The runtime cache lives at the app data directory under
`ocr/ddddocr/<sha256>.onnx`.

Set `SZU_LIFE_MODEL_CACHE` to use another build cache directory. For an offline
installer, set `SZU_LIFE_BUNDLE_OCR_MODEL=1` and use `tauri.conf.bundled.json`.
The build stages only a verified cache file into the bundle. Neither ONNX files
nor cache/download files belong in Git; `.gitignore` excludes them.

Example bundled build:

```sh
export SZU_LIFE_BUNDLE_OCR_MODEL=1
export TAURI_CONFIG="$(tr -d '\n' < src-tauri/tauri.conf.bundled.json)"
vpr tauri build --bundles appimage
```

When the bundled configuration is used, the verified bytes are included as a
native resource and released into the app data cache at startup. The default
configuration includes only the model manifest and license.

Upstream: https://github.com/sml2h3/ddddocr
Commit: c40f56f95412e10bcb9bd0bd24411e92f896d238
SHA-256: 33b5cd351ee94e73a6bf8fa18c415ed8b819b3ffd342e267c30d8ad8334e34e8
Size: 54,088,400 bytes
Runtime: ddddocr-core and ddddocr-tract2 0.2.4, MIT OR Apache-2.0.

Do not replace this with common_old.onnx, which is incompatible with this Tract
adapter. Update the model manifest, matching charset and inference test together.
