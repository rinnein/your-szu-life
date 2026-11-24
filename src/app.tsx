import React from "react";
import ReactDOM from "react-dom/client";
import { RouterProvider } from "@tanstack/react-router";
import { attachConsole } from "@tauri-apps/plugin-log";
import { router } from "./router";
// material 的样式以 JS 模块导出（模块内部 import 自己的 css），CSS 的 @import
// 无法解析它的 exports 条件，因此从入口引入。必须在 index.css 之前，
// 让应用的 @theme 映射覆盖基线变量。
import "material/styles/base";
import "material/styles/typography";
import { applyMaterialDefaults } from "./theme/material-defaults";
// MiSans 可变字重（家族名 "MiSans VF"）。misans 按 unicode-range 切片，
// 浏览器只下载页面实际用到的切片，整包 4.5MB 不会一次性进内存。
// 字体栈与其 MD3 覆盖写在 index.css 的 :root 里，这里只负责加载字形。
import "misans/lib/Normal/MiSansVF.min.css";
// 等宽字体 Monaspace Neon（只有 latin 子集，约 44KB/字重）。
// 只取 400/500/600/700，且只引用 woff2 版本（见 monospace.css）。
import "./monospace.css";
import "./index.css";

if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
  void attachConsole().catch(() => {});
}

applyMaterialDefaults();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <RouterProvider router={router} />
  </React.StrictMode>,
);
