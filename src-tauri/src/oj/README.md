# SZU OJ adapter

The student API is derived from the publicly served frontend of the supplied
SZU installations (inspected 2026-10-04). SZUOJ/JudgeServer is the judge worker,
not the account/class/contest API. The adapter uses the site's `/api/` prefix,
Django session cookies, `csrftoken`/`X-CSRFToken`, and offset/limit pagination.
No teacher/management API is used.

- `POST captcha/`: captcha_key, captcha_image (base64); the key stays native.
- `POST user/login/`: username, password, captcha_key, captcha_value.
- `GET user/profile/`: confirms the current username.
- `GET groups/`, `GET contests/?group=…`, `GET contests/{id}/`.
- Contest data: begin_time/end_time/now_time, group, problem_json keyed by
  numeric problem type, problem_score. Times are parsed with their UTC offsets.
- Types: 0 coding, 1 choice, 2 fill-in, 3 QA; 4 code completion when supplied.
- `POST code-submission/` / `code-fill-submission/`: code, language, problem,
  contest, options. Result lookup is GET of the returned ID.
- Non-coding answers use `contest-submission/` (POST on first submission,
  PATCH of the existing ID thereafter): choice_submission, fill_in_submission,
  qa_submission_list. This is the site's "submit non-coding paper" operation;
  it does not implicitly submit code or invent a final-lock operation.

Credentials and cookie jars are stored through the existing credential store.
Non-secret drafts, reminder state, and uncertain request journals are persisted
atomically under the app-data `oj/state.json`. Every native command validates
the local calling window. Connection generations cancel stale requests.
HTTP 401 permits one login recovery; generic 403/404 do not. Side-effecting
requests are never automatically retried. A request journal is written before
submission so an app crash also leaves a result requiring verification.

OCR uses the shared `crate::ocr::recognize` service and the model declared in
`../../resources/ocr/ddddocr/model.json`. Cargo downloads and verifies the model
at build time, then embeds it; no model or image is sent to an online solver.
The raster captcha fixture contains no account information or challenge key.
Model initialization and prediction run outside the asynchronous executor.

The desktop sender implements Competitive Companion's standard HTTP POST `/`
payload, not cph-ng's experimental rewrite RPC. It sends to exactly one explicit
loopback port, bypasses proxy settings, does not attach OJ cookies, and reports
partial batch failures without retrying. The test adapter covers whitespace,
unit conversion, fields, and batch identity. Real IDE receipt is a separate
acceptance step.

Validation: frontend tests include navigation/search parsing, configuration,
manual draft save failures, explicit submit confirmation, HTML sanitization,
Markdown content, and mobile action visibility. Rust tests include full model
inference, cookies/CSRF, account isolation, pagination failures, uncertain
submission persistence, reminders and protocol conversion. Browser QA uses
synthetic fixture data; no live account submission was performed. Final APK
installation, device gestures, notification delivery and authenticated site
variants must be validated independently.

## UI and protocol refinements

OJ is reached through the computing collection logical page. Visible labels are
assigned per problem type (A..Z, AA..) independently of numeric API IDs. Both single and bulk Companion sends use ASCII labels as names, because the
cph-ng default filename generator preserves Unicode letters from name. Full
titles remain in the app and Markdown. Duplicate labels across exported types are rejected rather than
silently targeting the same code file.

C defaults to C17, C++ to C++20. ASan and LSan default off; LSan requires ASan.
Options are typed and validated before persistence and submission. Existing
null/empty options are upgraded when read; valid saved standards are preserved.
Only C/C++ submissions include the server's snake_case options object.

Desktop uses lazy-loaded Monaco with locally bundled editor and TypeScript
workers; mobile uses CodeMirror. Operation success messages use HeroUI Toast
with a 3000 ms timeout. Persistent errors and unknown submission outcomes remain
visible until handled.

## Submit from cph-ng

Reference: https://github.com/langningchen/cph-ng/blob/main/docs/browser-extension/browser-extension.md

On desktop, the main app window acts as a CPH-NG Submit browser endpoint:
Socket.IO `/ws`, `type=browser`, `status.isActive`, `setActive`, and
`submitRequest { url, sourceCode }`. Compatibility is verified against the
installed cph-ng 0.7.11 router. The app does not automatically take over another
active browser; use "使用泥深提交" in the OJ connection panel when needed.
The receiver port must match cph-ng. The app must remain running (tray is OK).

Only exact configured instance URLs using the exported contest/problem link
format are accepted. The native service revalidates account generation, contest
access/time and allowed language, and reuses the normal submission journal.
It submits the IDE code directly without overwriting app drafts. Duplicate
in-flight/recent requests are rejected; uncertain writes are never retried.
Submission notices and result records are available in the app. The legacy
router does not define a browser-to-IDE judging-result event.

Preferences are saved atomically per instance, with the selected language and
separate options for each language. Changing them does not require a submission
or leaving the page. Old data without preferences derives an initial selection
from its most recently edited code draft; explicit instance preferences always
win afterwards. A problem that disallows the selected language prompts the user
instead of changing the preference. Other open OJ windows receive updates.

Socket.IO is a normal `socket.io-client` npm dependency locked by pnpm and
bundled locally by Vite. No runtime CDN dependency or custom Socket.IO
implementation is used. Its MIT license is included in native packages.
