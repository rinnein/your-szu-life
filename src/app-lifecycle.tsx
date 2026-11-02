import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * 应用生命周期信号桥。
 *
 * 这些状态原先散落在通知桥与电费监控 Provider 里，导致它们的挂载层级被
 * 业务范围绑架。集中到根节点后：
 * - `set_application_focus`：后端据此决定发应用内 Snackbar 还是系统通知；
 * - `set_application_visibility`：电费后台轮询据此区分前台/后台尝试。
 *
 * OJ 的前台刷新由后端在焦点事件里完成并推送 Channel，前端不再重复 invoke。
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

  return null;
}
