# Shared ddddocr model

All features use `crate::ocr::recognize`; the model and engine are not owned by
OJ. This directory contains only the pinned model manifest and its MIT license.

Cargo's `build.rs` prepares the model for every native build, including direct
`cargo check/test`, Tauri desktop builds and Android cross-compilation. It fetches
`common.onnx` from the immutable upstream commit in `model.json`, verifies both
size and SHA-256, and caches it under `src-tauri/target/model-cache/<sha256>.onnx`.
A valid cache is reused without network access. A file lock serializes parallel
builds; incomplete or invalid downloads never become the cached model.

Set `SZU_LIFE_MODEL_CACHE` to use another cache directory. For offline builds,
copy the verified model into that directory using its SHA-256 as the filename
followed by `.onnx`. The build verifies preseeded files too. Neither ONNX files
nor cache/download files belong in Git; `.gitignore` excludes them.

The verified bytes are still embedded in the native executable/APK, so end users
can recognize captchas offline without a model download or Python runtime.
The model's license and manifest are included in native packages.

Upstream: https://github.com/sml2h3/ddddocr
Commit: c40f56f95412e10bcb9bd0bd24411e92f896d238
SHA-256: 33b5cd351ee94e73a6bf8fa18c415ed8b819b3ffd342e267c30d8ad8334e34e8
Size: 54,088,400 bytes
Runtime: ddddocr-core and ddddocr-tract2 0.2.4, MIT OR Apache-2.0.

Do not replace this with common_old.onnx, which is incompatible with this Tract
adapter. Update the model manifest, matching charset and inference test together.
