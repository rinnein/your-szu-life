import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { isChildDocument } from "./logical-navigation";

/**
 * 应用生命周期信号桥。
 *
 * 这些状态原先散落在通知桥与电费监控 Provider 里，导致它们的挂载层级被
 * 业务范围绑架。集中到根节点后：
 * - `set_application_focus`：后端据此决定发应用内 Snackbar 还是系统通知；
 * - `set_application_visibility`：电费后台轮询据此区分前台/后台尝试；
 * - `refresh_oj`：回到前台时刷新一次 OJ 监测数据。
 */
export function AppLifecycleBridge() {
  useEffect(() => {
    if (typeof window === "undefined") return;

    const report = () => {
      const visible = document.visibilityState === "visible";
      const focused =
        visible && (typeof document.hasFocus === "function" ? document.hasFocus() : true);
      void invoke("set_application_focus", { focused }).catch(() => undefined);
      void invoke("set_application_visibility", { visible }).catch(() => undefined);
    };

    report();
    window.addEventListener("focus", report);
    window.addEventListener("blur", report);
    document.addEventListener("visibilitychange", report);
    return () => {
      window.removeEventListener("focus", report);
      window.removeEventListener("blur", report);
      document.removeEventListener("visibilitychange", report);
    };
  }, []);

  useEffect(() => {
    if (typeof window === "undefined" || isChildDocument()) return;
    if (!("__TAURI_INTERNALS__" in window)) return;

    const refresh = () => {
      if (document.visibilityState === "visible") void invoke("refresh_oj").catch(() => undefined);
    };
    document.addEventListener("visibilitychange", refresh);
    return () => document.removeEventListener("visibilitychange", refresh);
  }, []);

  return null;
}
