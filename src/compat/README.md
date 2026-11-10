# WebView compatibility

The local frontend supports Chromium 90 and newer. `vite.config.ts` shares the
same Oxc target with dependency optimization and copied Monaco modules. The
entry loads ECMAScript patches before importing the application; Workers carry
their own idempotent runtime.

`scripts/webview-compat.ts` processes application CSS and Material's registered
Shadow DOM styles. It retains native declarations and emits capability-gated
fallbacks. `:has()` attributes preserve selector specificity and are synchronized
within each DOM scope. Container conditions use measured container widths.
Independent transforms and spring curves have fallbacks that preserve media
conditions and do not turn `transform: none` into a containing block.

Cascade layers stay native: `@layer` blocks are never flattened for capable
engines. A flattened copy, scoped to `@supports not (selector(:has(*)))`, is
emitted right after each layer so engines without `@layer` still get its rules.
Engines that understand `@layer` therefore keep layer priority and the original
selector specificity.

Theme generation publishes RGB channels and common mixed colors alongside MD3
roles. Remaining mixes involving component-local variables or `currentColor`
are evaluated in their own scope, including pseudo-elements. Mutation observers
ignore compatibility writes and detach from removed roots.

Popover methods use `@oddbird/popover-polyfill` only when absent. Its emulated
top layer cannot escape clipping and stacking contexts, so affected Material
overlays temporarily move to an isolated Shadow Root. Existing event listeners,
focus targets and styles are retained; closing restores their original position.
Unconstrained overlays remain in place to retain `::part` styling.

Validation:

- `vp test run src/compat scripts/webview-compat.test.mjs scripts/modern-monaco-assets.test.mjs`
- `vp check` and `vp run build`
- Real Chromium 90 and current Chromium for controls, theme changes, overlay
  positioning, modal dismissal, container resizing and Monaco/Worker startup.
- ARM64-only APK build; installing the final APK on an old-WebView Android
  device remains a separate native acceptance step.

No MCP tool or resource is added: this changes client rendering and runtime
compatibility, without introducing or changing a business capability or API.
