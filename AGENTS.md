<!--VITE PLUS START-->

# Using Vite+, the Unified Toolchain for the Web

This project is using Vite+, a unified toolchain built on top of Vite, Rolldown, Vitest, tsdown, Oxlint, Oxfmt, and Vite Task. Vite+ wraps runtime management, package management, and frontend tooling in a single global CLI called `vp`. Vite+ is distinct from Vite, and it invokes Vite through `vp dev` and `vp build`. Run `vp help` to print a list of commands and `vp <command> --help` for information about a specific command.

Docs are local at `node_modules/vite-plus/docs` or online at https://viteplus.dev/guide/.

## Built-in Commands vs Scripts

`vp <name>` runs a built-in command. `vp run <name>` runs a `package.json` script or a `vite.config.ts` task. Scripts cannot overwrite built-ins, so `vp dev` and `vp run dev` may do different things. Check `package.json` and `vite.config.ts` first, and run `vp run <name>` when the project defines a script or task with that name.

## Running Scripts

Run TypeScript and JavaScript files with `vpx <file>`, for example `vpx ./scripts/seed.ts --dry-run`. It handles enums, decorators, JSX, and tsconfig `paths`, so do not add `tsx` or `ts-node` to run scripts. `vpx --watch <file>` reruns the file when a file it uses changes, in place of `tsx watch`. `vp node <file>` runs plain Node.js.

## Tool Versions

Run `vp toolchain` to show versions and relationships in the active Vite+
release. Add a tool name to select part of the graph. For example, run
`vp toolchain vite`. Use `--global` to ignore the local `vite-plus` package. Use
`vp why <package>` to show the package-manager dependency graph.

## Review Checklist

- [ ] Run `vp install` after pulling remote changes and before getting started.
- [ ] Run `vp check` and `vp test` to format, lint, type check and test changes.
- [ ] Check if there are `vite.config.ts` tasks or `package.json` scripts necessary for validation, run via `vp run <script>`.
- [ ] If setup, runtime, or package-manager behavior looks wrong, run `vp env doctor` and include its output when asking for help.

<!--VITE PLUS END-->

---

前端实现过程中，尽量使用 TanStack、material（floor/material，Material 3 Web Components）与 motion 等已有的库，最好不要自行通过写CSS和原生HTML来实现UI组件。
后端实现过程中，所有数据处理如果有成熟的处理库与框架的，最好向我询问是否安装这些库来简化实现。

---

前端实现时，任何通知、警告等，应当尽可能使用 `src/ui` 里封装好的 `toast`（用于显示某个操作的返回状态结果）、`Banner`（用于显示持久性的警告信息）、`AlertDialog`（用于显示需要用户确认的警告信息） 等组件，而最好不要通过 setXXXMessage() 等未封装的方法，alert()、console.log() 等原生方法来实现。

### 前端 UI 架构约定（Material 3）

- 唯一入口是 `src/ui`。业务代码只能从 `src/ui` 导入 UI 组件，不要直接 `import ... from "material/react"`，也不要新增第二套 UI 库。`src/ui` 内部才允许接触 material 的原始 API。
- 视觉令牌来自 MD3：`src/theme/` 用种子色生成 `--mtrl-sys-color-*`，`src/index.css` 的 `@theme inline` 把 `text-muted`、`border-separator`、`bg-surface` 等 Tailwind 工具类映射到这些角色上。写新样式时优先用语义类或 MD3 角色变量，不要再引入字面色值。
- 深色模式由 `data-theme-mode` 与 `color-scheme` 驱动；Android 端种子色由 Kotlin 桥读取壁纸色，桌面端可在设置页手动选色，二者互斥，用户手动选色后壁纸不再覆盖。
- material 的组件强制使用 shadow DOM，组件外观只能通过 CSS 自定义属性影响。需要给宿主加布局时用宿主标签（如 `m-button`）或外层容器，不要试图用选择器穿透 shadow root。
- 缺口组件（DataTable、Banner、Disclosure、Avatar、Kbd、Pagination、ComboBox、CheckboxGroup、ScrollShadow、NumberField、Toast 命令式 API、ProgressCircle）在 `src/ui` 内自建，保留原有复合 API 形状以降低迁移成本；不要为了这些去引入第二套组件库。
- 页面顶栏是 `src/shell/TopAppBar.tsx` 里对 material `m-top-app-bar` 的组合：返回键、标题、搜索与页面动作都以 light DOM 放进 `leading`/`trailing`/标题槽位，因此它们仍能带 `data-webview-navigation`（material 自己的 IconButton 把真实按钮放进 shadow root，做不到）。顶栏壳子默认 `position: absolute`、固定 64px，已在 `src/index.css` 里用 `::part(top-app-bar)` 覆盖成静态定位；外层 `.top-app-bar` 用 `position: sticky` 吸顶、并用 `margin-inline: calc(50% - 50cqw)` 横向撑满内容区（`cqw` 基准是 `.app-content` 的 `container-type: inline-size`，代价是内容区内的 `position: fixed` 元素改为相对内容区定位——浮动层请用 `createPortal(..., document.body)`）。该撑满依赖「顶栏祖先链水平居中且无横向内边距」，新增页面不要把顶栏放进别的容器里。滚动状态交给 material 自己按 `window.scrollY` 驱动，不要传 `no-scroll` 或调 `setScrollState`。顶级部件页（首页/学习/生活/设置）用 `showBack={false}`，只有子页面给返回键。
- 动效统一走 `src/motion/`（motion 库）与 MD3 的弹簧参数；material 自带动效的组件不要叠加二次动画。页面切换动画只在 `push`/`back` 时启用，tab 切换（`replace`）不动画。
- Android 的原生浏览器栏（`NativeWebViewChrome` 自绘的 `MaterialToolbar`）**只在外部页面出现**：统一认证、自助服务、官方网费等受控打开的非 Tauri 本地页面才需要它提供标题与返回/刷新；本地页面一律由 Web 顶栏负责，因此 Android 本地页面不会收到 `native-toolbar-ready`，也就不会再隐藏页内的 `data-webview-navigation` 控件。判定依据是当前 document 的来源是否属于本地 origin（`ApplicationBackPolicy.localDocument` → `shouldShowBrowserToolbar`），切换页面时随 `document` 通知即时显隐。
- Android 的状态栏与「小白条」（手势导航条）共用同一套沉浸式逻辑：上下两条 inset 区域各由一块占位视图（`NativeWebViewChrome` 的 `statusBarBand` / `navigationBarBand`）绘制，因为 Android 15 起 `window.statusBarColor` / `navigationBarColor` 会被系统忽略、边到边模式下只能自己画；旧版本系统会用不透明系统栏盖住这两块，效果一致。`SystemBars.apply(window, statusBar, navigationBar)` 一个入口给两条栏上色并一起决定图标明暗，内容区留白由 `NativeWebViewChrome` 的 insets 监听统一处理（内容不画到系统栏下面，栏上的颜色就是应用自己画的那条带状区域；Android 15 起 `statusBarColor` 被系统忽略时由该区域兜住）。取色规则：**状态栏取「顶部条」当前的颜色**——本地页面是 Web 顶栏（滚动到一定位置会从 `surface` 变成 `surface-container`，因此随滚动变化），外部页面是原生浏览器栏（取其主题色）；**小白条固定取底部导航栏的颜色**，不随滚动变化。Web 端在 `src/shell/native-chrome.ts` 里**读控件的 computed style**得到颜色（不在前端重复 M3 令牌值），用 `chrome-status-bar:#rrggbb` / `chrome-nav-bar:#rrggbb` 两条消息推送，并且只在颜色真的变了时才发——滚动每一帧都发会刷爆原生侧。原生侧解析失败或还没收到推送时退回主题的 `colorSurface` / `colorSurfaceContainer`。
- 日志插件的 `Webview` 目标是「每条 Rust 日志过一次 JNI 转发」，因此**不要**给它或 `Stdout` 放开 DEBUG/TRACE：`jni` crate 会为转发本身的 JNI 调用再记一条日志，两者互相放大，真机实测几秒内刷出每秒上万行并把主线程压到 ANR。现状是 `src-tauri/src/logging.rs` 的 `quiet_targets` 只放行 Info 及以上、并在 `app.rs` 里关掉 `jni`/`tao`/`wry` 等噪声库；逐条的 DEBUG/TRACE 应用日志仍然写进日志文件（由用户的保留级别控制）。
- 测试里 material 控件的角色与可访问名称在 shadow root 内部：全局补丁 `src/test-setup-element-internals.ts` 让 `getByRole` 能穿透 open shadow root；输入请用 `src/ui/testing.ts` 的 `typeInto`，断言可见文案请用 `hostOf` 取宿主元素。

---

使用cargo/tauri指令进行任何测试或构建时，除非手动明确之处执行，否则必须遵循以下关于架构选择的注意事项：

- 桌面端构建仅对当前平台进行构建
- Android构建仅对 aarch64 架构进行apk构建，不要构建universe包
- 进行测试时，需要考虑对 桌面端(Linux/Windows)/Android 的平台指定代码，保证CI中需要构建的平台能够全部通过cargo test的基本构建
- 不要 allow(dead_code) 或 allow(unused) 来绕过检查，需要明确确定是真的没用(直接删掉)或是特定平台测试专用(配置好cfg)

---

写测试/执行测试时，如果发现有耗时极长的情况：

1. 先检查测试逻辑是否正确，是否有死循环或无限等待的情况，如果是测试逻辑问题，先修复测试逻辑再执行测试。
2. 如果测试逻辑没有问题，检查是否是因为测试中使用了网络请求或其他外部依赖导致的耗时过长，如果是这种情况，考虑使用mock或stub来替代真实的网络请求或外部依赖。
3. 如果测试逻辑和外部依赖都没有问题，检查是否是因为测试数据量过大导致的耗时过长，如果是这种情况，考虑使用更小的数据集或优化测试数据的生成方式。
4. 如果以上都没有问题，考虑使用异步测试或并行测试来提高测试效率，或者将耗时的测试拆分成多个小的测试用例。
5. 如果还是有超时，但测试本身具有不可代替的必要性，则默认允许在测试中使用 `#[ignore]` 来跳过该测试，但必须在测试函数上添加明确的注释说明为什么需要忽略该测试，以及在什么情况下可以取消忽略。

---

## 功能模块 MCP 服务规范与检查清单

本项目为所有核心业务能力提供本地公开的 Model Context Protocol (MCP) 服务，支持外部大模型客户端（如 Cursor、Claude Desktop 等）及内部 `rig-core` 智能体调用。

在开发、重构或新增任何业务功能模块时，必须遵守以下规范：

### 1. 强制同步检查规则

- **新增业务功能时**：除了提供前端 UI、路由与 Tauri commands 之外，**必须同步检查并提供对应的 MCP 工具（Tool）或资源（Resource）实现**。若该功能不需要对 AI 开放，必须在代码提交说明中显式给出技术理由。
- **修改现有功能接口时**：若变更了数据结构或业务逻辑，必须同步更新该功能模块下 `mcp.rs` 中的工具实现、参数 Schema 与文档描述。

### 2. 实现与技术选型约定

- **协议框架**：统一使用 `rmcp` 宏体系（`#[rmcp::tool]` 与 `#[rmcp::tool_router]`），参数统一封装于 `rmcp::handler::server::wrapper::Parameters<T>`。
- **端口与安全限制**：MCP 本地公开服务必须使用**高位端口**（默认 `53901`，动态端口范围），且**仅监听本地回环地址 `127.0.0.1` / `localhost`**，绝对禁止对外网或局域网广播。
- **协议 URI Scheme 规范**：所有通过协议暴露的端点统一注册为 `your-szu-life://`，并严格区分两大 route 空间：
  - `your-szu-life://page/...`：用于控制客户端打开/激活特定前端页面（如 `page/workbench`, `page/auth`, `page/electricity` 等）。
  - `your-szu-life://data/...`：用于传递与订阅结构化数据/MCP Resources 资源（如 `data/auth/profile`, `data/electricity/status` 等）。
- **类型契约**：所有工具入参和出参必须 `derive(Serialize, Deserialize, schemars::JsonSchema)`，并提供清晰详尽的中文描述字段，杜绝任何未定义结构或无提示的 JSON Value。
- **子路由导出**：每个功能模块目录内建立 `mcp.rs`，导出命名的 `pub(crate) fn xxx_tools(...) -> ToolRouter`，并在 `src-tauri/src/mcp/server.rs` 中合并接入全局路由器。
- **内部智能体互通**：所有 MCP 工具必须兼容 `rig-rmcp`，确保应用内置的 `rig-core` 智能体能够通过 `tools_from_server` 直接动态挂载并使用。

### 3. 开发验收检查清单 (Checklist)

每次提交功能前，逐项检查：

- [ ] 对应模块下已创建或更新 `mcp.rs` 并完成工具注册。
- [ ] 工具名称符合命名规范（`<module>_<action>`，如 `uooc_search_qa_bank`）。
- [ ] 工具入参均已实现 `schemars::JsonSchema`，且字段具有明确的注释描述。
- [ ] 若涉及页面跳转或数据快照，已分别在 `your-szu-life://page/...` 与 `your-szu-life://data/...` 下补充对应 URI 契约。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml` 验证 MCP 单元测试与 Schema 生成。
- [ ] 本地 MCP 服务（HTTP/SSE 与 stdio）能够正常返回该工具的调用结果。

---

## 逻辑页面与返回行为

本项目的“返回”语义以**应用级逻辑导航栈**为准，不以 WebView 的 `window.history` 为准。实现或修改页面导航时遵循以下约定：

### 页面层级

- 首页（`/`）是逻辑导航栈的 root。
- 顶级部件只有四个，宽屏 rail 与窄屏 bar 显示完全相同的入口：首页 `/`、学习 `/study`、生活 `/life`、设置 `/settings`。
- 子页面在路径上带归属前缀：`/study/uooc`、`/study/oj`、`/study/computing`、`/study/class-timetable`、`/study/academic`、`/study/enrollment/...`、`/life/campus-network`、`/life/campus-payments/...`。返回层级由路径层级决定（例如从 `/study/uooc` 返回落到 `/study`）。
- 「关于」与「诊断日志」不是独立路由，而是设置页的 tab：`/settings?tab=about`、`/settings?tab=debug`（后者仅在调试模式启用后出现）。
- WebVPN 没有独立页面与入口：需要校外访问的操作在发起前探测，只在确实需要 2FA 时弹出确认 Dialog。
- 功能模块内部通过 tab、筛选器或步骤切换内容，仍属于同一个逻辑页面；这些切换不应制造新的返回层级。
- 功能模块内部再次打开的独立页面或外部门户（例如官方网费页面）才建立新的子逻辑页面或独立窗口。

### 导航实现规则

- 统一通过逻辑导航 API（如 `LogicalNavigationProvider` / `useLogicalNavigation()`）执行 `push`、`replace`、`back` 和 `canBack`，不要在业务组件中直接依赖 `window.history.back()`。
- 页面内部 tab 或同一功能页的 URL/状态切换使用 `replace`；需要形成新的页面层级时才使用 `push`。
- TanStack Router 的 WebView URL 同步应使用替换语义（`replaceState` 或 memory history），避免把逻辑页面切换写入真实 WebView history。
- 深链接进入功能页时，应补齐逻辑栈上下文（由路径层级推出：`[/, /study, /study/enrollment, 当前页面]`），使返回仍能回到上一层级。
- 单 WebView 内打开网费 fallback 页面时使用 `location.replace()`；不要通过 `location.href` 或普通链接额外堆积 WebView history。
- 需要独立生命周期、取消任务或对应 Android Activity 的外部/门户页面，使用独立 `WebviewWindow` / `FeatureActivity`；关闭后应回到其父逻辑页面并清理关联任务。

### 返回入口

- 桌面窗口关闭、Android 系统返回键、应用内页面返回按钮必须调用同一套逻辑返回桥接，保持行为一致。
- 返回按钮应绑定 HeroUI 等现有组件的标准事件（例如 `onPress`），并确认事件链路最终到达原生关闭/返回实现；不要只验证函数被调用。
- 当逻辑栈没有可返回页面时，才执行 root 页面或当前原生窗口的默认退出行为。
- 不要把“调用了 `back()`”等同于“Activity 已关闭”；Android 必须验证桥接、Activity/窗口状态和前台页面。

### 修改与验收清单

1. 先判断变更是同一逻辑页面内的状态替换，还是新的逻辑页面；据此选择 `replace` 或 `push`。
2. 检查桌面、Android 和页面按钮是否都复用统一返回桥接。
3. 为逻辑栈、深链接、tab 替换、独立门户窗口和返回边界补充测试。
4. 至少运行 `vp check`、`vp test`、`vp run build`、`cargo test --manifest-path src-tauri/Cargo.toml` 以及相关 Android 编译检查。
5. 真机 Android 返回键、页面内返回按钮和真实账户门户流程属于独立的手工验收；未安装并点击最终 APK 前，不得声称原生返回已验证。
6. 若 Android lint 报 `EnsureInitializerMetadata`，检查生成的 `SplitInitializer` 与 `AndroidManifest.xml` 的 `<meta-data>` 配对，不要用无关的 lint baseline 掩盖问题。

当前参考实现已经验证了应用级逻辑导航栈、memory history、tab `replace`、深链接 root 补齐、独立网费窗口及统一返回桥接；但此前记录中 Android 真机返回键和真实网费账户流程仍需单独验收，后续工作应明确区分自动化检查、原生设备验收和真实账户验收。
