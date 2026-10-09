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
IDE code is atomically cached in the current account/problem/language draft before
network access, including failed submissions. Open editors receive `oj-draft-changed`;
unsaved local edits require choosing which version to keep. Duplicate
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

## Submission presentation

`get_oj_records` and `get_oj_result` accept an optional `problemId`. Code/QA
records send the API problem filter and also filter locally after full pagination.
Choice/fill answers are projected out of the whole-paper record; unrelated answers
are not returned. Directory verdicts use current-account submissions, keep AC once
solved, and otherwise show the most recent attempt.

Result details follow the public SZU frontend checked on 2026-10-09:
`GET /api/code-submission/{id}/` returns `status`, `time`, and `result.data`
with `test_case`, `result`, `cpu_time` and `real_time` (ms), and `memory` (bytes).
CE/SE can return a string in `result`. The UI uses status chips, visible-case
progress and a result table; it never queries teacher test-data endpoints.
Active result views poll sequentially every 2 seconds, pause after three read
failures or 90 attempts, and cancel on problem/account changes or unmount.
Pending latest submissions are resumed when reopening a problem. No polling
path repeats a submission. Samples are read-only fields and copied from the
original strings without trimming; only the Companion wire format appends its
required final newline when absent.

## Verdicts, completion and ranking

Known verdicts (-3 through 8) colour the whole directory button using semantic
backgrounds; selection is an independent ring. All status chips use filled
backgrounds. Group completion requires every visible problem in that group to
be AC; contest completion requires all visible problems to be AC (an empty
contest never completes). Non-coding saved answers are not treated as AC.
The programming heading shows its AC count over the contest's total question
count, and the detail deadline shares the server-adjusted countdown.

`get_oj_progress` reads contest problem IDs and the student's
`/api/contests/{id}/self_rank/` accepted flags, with separate code-fill records
where needed. Completion results are shared by the application rather than reloaded on focus.
`get_oj_ranking` reads `/api/contests/{id}/rank/?offset=…&limit=20`, preserves
server order and projects only public ranking fields. `show_ranking: false`
and HTTP permission errors stay inline; no roster/teacher endpoint is used.

Submission code and results are separate, initially collapsed HeroUI Accordion
panels. Code editors are mounted only while expanded and are read-only.
Desktop uses `modern-monaco` 0.4.2; mobile retains CodeMirror. The npm editor,
worker modules, themes and grammar dependencies are served and copied by the
Vite static-copy plugin. The upstream JS language-service worker is bundled
with Rolldown and TypeScript 6 under the `typescript-lsp` alias, because the
app's TypeScript 7 toolchain is native and has no browser compiler module.
The application's TypeScript compiler remains unchanged. No runtime CDN is
needed for OJ languages, syntax, snippets, the editor or its built-in JS service.

## Global completion cache

TanStack React Store (`createStore` / `useSelector`) owns the completion state
shared by the contest list and detail page. Entries are scoped by site ID,
normalized URL, account and native `sessionGeneration`, then contest ID. Successful
results stay fresh for five minutes; remounts and changed list array identities
reuse them, and in-flight requests are shared across React callers. Focus and
visibility changes do not force a progress scan. Expired entries keep the previous
label during refresh; failures keep that label and back off for 30 seconds. The
manual OJ refresh invalidates the displayed contest IDs and forces a server read.
Complete submission lists already read by the problem page directly seed the store.

Each native account session also caches progress for five minutes, so independent
Android/WebView pages reuse the same network result. The account gate coalesces
concurrent reads across windows. Credential replacement/deletion discards that
session's cache. `get_oj_progress` accepts `force`, `account`, `sessionGeneration`
and returns `updatedAt` alongside accepted/total counts. Submissions and changed
terminal verdicts emit `oj-progress-invalidated` for just the affected contest;
the root bridge updates every WebView store. Late replies cannot overwrite a
newer locally derived result or recreate a removed account's cache. This is an
application-lifetime cache, not credential or restart persistence.
