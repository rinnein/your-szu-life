<!--VITE PLUS START-->

# Using Vite+, the Unified Toolchain for the Web

This project is using Vite+, a unified toolchain built on top of Vite, Rolldown, Vitest, tsdown, Oxlint, Oxfmt, and Vite Task. Vite+ wraps runtime management, package management, and frontend tooling in a single global CLI called `vp`. Vite+ is distinct from Vite, and it invokes Vite through `vp dev` and `vp build`. Run `vp help` to print a list of commands and `vp <command> --help` for information about a specific command.

Docs are local at `node_modules/vite-plus/docs` or online at https://viteplus.dev/guide/.

## Built-in Commands vs Scripts

`vp <name>` runs a built-in command. `vp run <name>` runs a `package.json` script or a `vite.config.ts` task. Scripts cannot overwrite built-ins, so `vp dev` and `vp run dev` may do different things. Check `package.json` and `vite.config.ts` first, and run `vp run <name>` when the project defines a script or task with that name.

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

前端实现过程中，尽量使用 TanStack、Heroui 等已有的库，最好不要自行通过写CSS和原生HTML来实现UI组件。
后端实现过程中，所有数据处理如果有成熟的处理库与框架的，最好向我询问是否安装这些库来简化实现。

---

## 逻辑页面与返回行为

本项目的“返回”语义以**应用级逻辑导航栈**为准，不以 WebView 的 `window.history` 为准。实现或修改页面导航时遵循以下约定：

### 页面层级

- 工作台（Workbench）是逻辑导航栈的 root。
- 从工作台打开的功能模块是子逻辑页面，例如校园卡、电费、网费、校园网、设置、公告、诊断日志等。
- 功能模块内部通过 tab、筛选器或步骤切换内容，仍属于同一个逻辑页面；这些切换不应制造新的返回层级。
- 功能模块内部再次打开的独立页面或外部门户（例如官方网费页面）才建立新的子逻辑页面或独立窗口。

### 导航实现规则

- 统一通过逻辑导航 API（如 `LogicalNavigationProvider` / `useLogicalNavigation()`）执行 `push`、`replace`、`back` 和 `canBack`，不要在业务组件中直接依赖 `window.history.back()`。
- 页面内部 tab 或同一功能页的 URL/状态切换使用 `replace`；需要形成新的页面层级时才使用 `push`。
- TanStack Router 的 WebView URL 同步应使用替换语义（`replaceState` 或 memory history），避免把逻辑页面切换写入真实 WebView history。
- 深链接进入功能页时，应补齐逻辑栈上下文（通常为 `[工作台, 当前页面]`），使返回仍能回到工作台。
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
