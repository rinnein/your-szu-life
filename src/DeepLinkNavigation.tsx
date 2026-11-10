import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { setupDeepLinkListener } from "./deep-link";
import { isChildDocument, useLogicalNavigation } from "./logical-navigation";

/** 与 Rust 侧 `desktop::TRAY_OPEN_ROUTE_EVENT` 对应的托盘「打开待办」事件。 */
const TRAY_OPEN_ROUTE_EVENT = "tray-open-route";

export function DeepLinkNavigation() {
  const { push } = useLogicalNavigation();
  const child = isChildDocument();

  useEffect(() => {
    // 深链接事件是应用级广播，子窗口各自响应会重复压入逻辑栈。
    if (child) return;
    let cleanup: (() => void) | undefined;
    void setupDeepLinkListener((route) => {
      push(route);
    }).then((c) => {
      cleanup = c;
    });

    return () => {
      cleanup?.();
    };
  }, [push, child]);

  useEffect(() => {
    // 托盘待办点击只投递到主窗口；复用 push 让目标逻辑页面以独立窗口打开。
    if (child || !("__TAURI_INTERNALS__" in window)) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<{ route?: string }>(TRAY_OPEN_ROUTE_EVENT, (event) => {
      const route = event.payload?.route;
      if (typeof route === "string" && route.startsWith("/")) void push(route);
    }).then((value) => {
      if (disposed) value();
      else unlisten = value;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [push, child]);

  return null;
}
