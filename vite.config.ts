import { defineConfig, lazyPlugins } from "vite-plus";
import react from "@vitejs/plugin-react";
import { tanstackRouter } from "@tanstack/router-plugin/vite";
import tailwindcss from "@tailwindcss/vite";
import { modernMonacoAssets } from "./scripts/modern-monaco-assets";
import { compatibilityPostcssPlugins, materialCompatibilityStyles } from "./scripts/webview-compat";
import { WEBVIEW_TARGET } from "./src/compat/contracts";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  oxc: { target: WEBVIEW_TARGET },
  build: { target: WEBVIEW_TARGET, cssTarget: WEBVIEW_TARGET, cssMinify: "lightningcss" },
  optimizeDeps: {
    // 只扫描应用入口；默认递归扫描 HTML 会把 output/playwright 中的浏览器资源也算进去。
    entries: ["index.html"],
    exclude: ["material"],
    rolldownOptions: { transform: { target: WEBVIEW_TARGET } },
  },
  css: {
    transformer: "postcss",
    postcss: { plugins: compatibilityPostcssPlugins() },
    lightningcss: { targets: { chrome: 90 << 16 } },
  },
  plugins: lazyPlugins(() => [
    materialCompatibilityStyles(),
    tanstackRouter({ target: "react", autoCodeSplitting: true }),
    react(),
    tailwindcss(),
    ...modernMonacoAssets(),
  ]),

  test: {
    // jsdom 没有实现 form-associated custom element 那半套 ElementInternals，
    // material 的每个表单控件在挂载时都会调用 setFormValue，缺了会直接抛错。
    setupFiles: ["./src/test-setup-element-internals.ts"],
    // 整套测试几乎都是 jsdom + 自定义元素。并行度拉满时 CPU 争用会让
    // 元素内部的事件派发与 React 的批次更新错位，出现「点了没反应」的偶发
    // 失败（换成 6 个 worker 后连续多轮全绿，总耗时只多约 1 秒）。
    maxWorkers: 6,
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 原生项目与浏览器验收产物不参与前端文件监听。
      ignored: ["**/src-tauri/**", "**/output/playwright/**"],
    },
  },
}));
