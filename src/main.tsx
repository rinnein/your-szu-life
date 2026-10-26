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
import "./index.css";

if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
  void attachConsole().catch(() => {});
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <RouterProvider router={router} />
  </React.StrictMode>,
);
