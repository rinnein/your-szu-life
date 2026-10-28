import { useRouterState } from "@tanstack/react-router";
import { type ReactNode, useLayoutEffect, useRef } from "react";
import { useLogicalNavigation } from "../logical-navigation";

/** 入场方向；`replace`（tab、筛选器）不播放动画。 */
type Direction = "forward" | "back" | "none";

function directionOf(action: string | null): Direction {
  if (action === "replace") return "none";
  return action === "back" ? "back" : "forward";
}

/**
 * 页面过渡。
 *
 * M3 Expressive 的转场只在**建立或回退页面层级**时播放（push / back）；
 * 同一逻辑页面内的 tab、筛选器切换走 `replace`，不播放动画 ——
 * 否则每次切 tab 都会整页滑入，观感与 Android 原生不符。
 *
 * 不靠给包裹元素换 `key` 来重播动画：换 key 会在同一批次里卸载整棵路由
 * 子树，打断正在进行的 history 回退（表现为点返回后停在原页，见
 * src/logical-navigation.tsx 的 `back`）。改为在同一次 CSS 动画上做一次
 * 强制回流把它从头播一遍，React 的挂载/卸载时机完全不参与。
 *
 * 尊重 `prefers-reduced-motion`（规则在 index.css）。
 */
export function PageTransition({ children }: { children: ReactNode }) {
  const { lastAction } = useLogicalNavigation();
  // 用完整 href 而不是 pathname：设置页这类同一路径下只换 query 的
  // `replace` 也要被观察到（虽然它不播放动画）。
  const href = useRouterState({ select: (state) => state.location.href });
  const direction = directionOf(lastAction());
  const ref = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    // 先复位再设回，让同一个 CSS 动画从头播一次。
    element.dataset.motion = "none";
    if (direction === "none") return;
    void element.offsetWidth;
    element.dataset.motion = direction;
  }, [href, direction]);

  return (
    <div ref={ref} className="page-transition">
      {children}
    </div>
  );
}
