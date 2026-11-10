import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { isChildDocument } from "../logical-navigation";

/**
 * feature 子窗口（桌面端独立打开的逻辑页面）的窗口标题跟随路由标题：
 * 路由标题由 TanStack 写入 `document.title`，桌面端不会自动同步到窗口标题，
 * 这里监听 `<title>` 变化后调用 `setTitle`。Android 的子窗口是原生
 * FeatureActivity，标题由原生栏负责，不做同步。
 *
 * 返回停止监听的清理函数；非桌面或非子窗口时返回空操作。
 */
export function startChildWindowTitleSync(): () => void {
  if (!("__TAURI_INTERNALS__" in window) || window.SzuApplicationBack || !isChildDocument()) {
    return () => {};
  }
  const webview = getCurrentWebviewWindow();
  let current = document.title;
  const apply = () => {
    if (document.title === current) return;
    current = document.title;
    void webview.setTitle(current).catch(() => {});
  };
  apply();
  const observer = new MutationObserver(apply);
  const title = document.head?.querySelector("title");
  if (title) observer.observe(title, { childList: true, characterData: true, subtree: true });
  return () => observer.disconnect();
}
