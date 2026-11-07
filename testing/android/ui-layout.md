# Android 界面回归

`scripts/ui-layout-check.mjs` 在独立浏览器中加载构建后的前端，注入 `ui-fixture.js` 的虚拟账户、初始化状态、公告和待办。测试不访问真实认证页，不保存账户、宿舍或应用设置；只能指定本机回环地址的预览服务。

## 执行

需要项目的 Vite+ / Node.js 运行环境，以及支持 Chrome DevTools Protocol 的 Chromium。旧内核回归使用 Chromium 92，现代浏览器另跑一遍；修改 User-Agent 不能代替旧内核测试。

先构建并在一个终端启动预览：

```bash
vp install --frozen-lockfile
vp run build
vp preview --host 127.0.0.1 --port 18422
```

在另一终端启动专用浏览器，不复用日常浏览器的用户目录：

```bash
SZU_UI_CHROME=/path/to/chromium
"$SZU_UI_CHROME" --headless --disable-gpu \
  --remote-debugging-address=127.0.0.1 --remote-debugging-port=18423 \
  --user-data-dir="$(mktemp -d /tmp/szu-ui-browser.XXXXXX)" about:blank
```

执行检查：

```bash
vp node scripts/ui-layout-check.mjs 18423 \
  .android-test/reports/ui-layout http://127.0.0.1:18422
```

端口被占用时为自己的服务选择其他端口，并更新参数。远程共享主机应使用已配置的独立资源限制和私有浏览器，不停止其他开发者的进程。结果和截图留在报告目录，不上传真实账户截图。

## 覆盖范围

25 个场景覆盖首页、学习、生活、设置和初始化的七个步骤，并增加 320px 小屏、1200px 宽屏、深色模式、大字体、横屏及输入法收起可视区域的情况。另含浅色/深色的六类待办标签、从生活页点击校园服务入口，以及校园卡/网费/电量/充值四个面板。电费样本包含读数与充值记录，用于实际加载 Recharts，不能仅验证空配置页。输入法场景通过缩小视口模拟，Android 原生输入法仍需在 APK 中验收。

检查包括内容没有横向溢出、日历保留七列、日期实际点击后有可辨认的选中色、登录按钮可见且点击触发表单校验、宿舍菜单留在视口内且选项实际点击后生效、设置分类菜单位于底栏上方且可切换外观页。待办标签必须包含文字且与背景异色；电费入口和详情页必须实际渲染图表。点击通过 CDP 的输入事件发送，避免仅调用元素的 `click()` 方法而漏掉遮挡。

每个场景使用独立的 browser context，防止上个页面保存的服务 tab、主题或向导状态污染后续场景。CDP Page 域先启用，再注册虚拟数据脚本。准备点击时使用即时滚动，避免在平滑滚动途中点击到正在移动的底栏。

`results.json` 记录实际浏览器版本、视口、元素位置和通过状态，任一场景失败时脚本返回非零退出码；每个场景同时生成 PNG 截图。

这组数据覆盖页面排版和交互。实际账号登录、校园业务服务及系统返回桥接应分别验收，不能用虚拟数据的浏览器检查代替。

## 修复的兼容性约束

- 构建将 Tailwind 与 material 的全局 CSS 分层、嵌套规则转换为 Chrome 92 可解析的形式。CSS 采用单一构建产物，保留全局 cascade layer 的优先级。
- 在 material 元素连接之前补充旧浏览器的 Popover API，让选择菜单保留 shadow root 内的样式。
- 内容区不建立 layout containment；顶栏宽度、初始化操作栏高度分别用 ResizeObserver 测量。
- 设置分类菜单通过 portal 挂到 body，并使用原生或兼容的 popover；根据实际视口和底栏调整位置。
- `runtime-compat.ts` 在 React/router 之前补充缺失的 `Object.hasOwn`，保留可用的原生实现。JavaScript 构建目标只转换语法，不会自动提供依赖调用的新 API。
- 标签背景色在生成两套主题时计算为不透明 sRGB 混合值，随种子色与深浅模式更新；不依赖旧 WebView 不支持的 `color-mix()` 或编译器为动态变量生成的近似回退。

华为 NAM-AL00 的实际 WebView 是 Chrome 92.0.4515.105。Chromium 92.0.4498.0 可用于同代 CSS / Popover 能力回归，但不等同于华为系统 WebView 或真实账户验收。
