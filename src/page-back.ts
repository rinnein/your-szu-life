import { useEffect } from "react";

/**
 * 页面内部的「返回上一步」。
 *
 * 逻辑页可以有子状态（如 OJ 的答题页），它们不占逻辑栈层级，但返回键应先把
 * 子状态收回列表。处理器由当前页注册，顶栏返回与 Android 系统返回共用同一个。
 */
export type PageBackHandler = () => void | Promise<void>;

let current: PageBackHandler | undefined;
const listeners = new Set<() => void>();

function notify() {
  for (const listener of listeners) listener();
}

export function setPageBackHandler(handler: PageBackHandler | undefined) {
  if (current === handler) return;
  current = handler;
  notify();
}

export function currentPageBackHandler() {
  return current;
}

export function subscribePageBackHandler(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/**
 * 为当前页注册页内返回处理器；未注册或处理器注销时，返回落到逻辑栈。
 *
 * 注册变化会通知 Android 桥重新发布 `route` / `system`：详情页展开时系统把
 * 返回手势交回页面，收回列表后下一次手势才由原生关闭子窗口。
 */
export function usePageBackHandler(handler: PageBackHandler | undefined) {
  useEffect(() => {
    if (!handler) return;
    setPageBackHandler(handler);
    return () => {
      if (current === handler) setPageBackHandler(undefined);
    };
  }, [handler]);
}
