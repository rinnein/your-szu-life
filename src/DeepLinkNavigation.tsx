import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { setupDeepLinkListener } from "./deep-link";
import { isChildDocument, useLogicalNavigation } from "./logical-navigation";
import { pairLoginAcceptUrl } from "./pair-login/api";
import { toast } from "./ui";
import { errorMessage } from "./async-state";

/** 与 Rust 侧 `desktop::TRAY_OPEN_ROUTE_EVENT` 对应的托盘「打开待办」事件。 */
const TRAY_OPEN_ROUTE_EVENT = "tray-open-route";

/** 远程授权页的固定路由；邀请密钥不进入路由，只通过 Rust 内存传递。 */
const PAIR_ROUTE = "/pair-login";

function isPairRoute(route: string) {
  const path = route.split("?")[0].replace(/\/+$/, "");
  return path === PAIR_ROUTE;
}

export function DeepLinkNavigation() {
  const { push, replace } = useLogicalNavigation();
  const child = isChildDocument();

  useEffect(() => {
    // 深链接事件是应用级广播，子窗口各自响应会重复压入逻辑栈。
    if (child) return;
    let cleanup: (() => void) | undefined;

    const acceptPairInvite = async (route: string) => {
      try {
        const state = await invoke<Record<string, unknown>>("get_auth_state");
        if (!("Authenticated" in state)) {
          toast.warning("请先在本机完成统一认证", {
            description: "只有已登录的设备才能授权另一台设备完成登录。",
          });
          return;
        }
        await pairLoginAcceptUrl(route);
        await push(PAIR_ROUTE);
      } catch (error) {
        toast.danger("无法打开授权页面", { description: errorMessage(error) });
      }
    };

    void setupDeepLinkListener((route) => {
      // 配对邀请：先把 secret 收进 Rust 内存，再打开不带 query 的授权页。
      if (isPairRoute(route)) {
        void acceptPairInvite(route);
        return;
      }
      // 根路由（课表提醒的「回首页」）要在当前窗口内替换：Android 上 push 会
      // 走 openLogicalWindow 新开一个首页子窗口，而不是把主窗口退回首页。
      if (route === "/") void replace("/");
      else void push(route);
    }).then((c) => {
      cleanup = c;
    });

    return () => {
      cleanup?.();
    };
  }, [push, replace, child]);

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
