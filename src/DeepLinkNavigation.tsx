import { useEffect } from "react";
import { setupDeepLinkListener } from "./deep-link";
import { isChildDocument, useLogicalNavigation } from "./logical-navigation";

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

  return null;
}
