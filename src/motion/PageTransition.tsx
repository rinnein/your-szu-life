import { useRouterState } from "@tanstack/react-router";
import type { ReactNode } from "react";
import { useLogicalNavigation } from "../logical-navigation";

/**
 * 页面过渡。
 *
 * M3 Expressive 的转场只在**建立或回退页面层级**时播放（push / back）；
 * 同一逻辑页面内的 tab、筛选器切换走 `replace`，不播放动画 ——
 * 否则每次切 tab 都会整页滑入，观感与 Android 原生不符。
 *
 * 用纯 CSS 动画而不是 motion 的 `motion.div`：后者以路由路径为 key，
 * 返回时在同一批次里卸载整棵子树，会打断正在进行的 history 回退
 * （表现为点返回后停在原页）。CSS 动画只作用于入场，不参与 React
 * 的挂载/卸载时机，因此不干扰导航。
 *
 * 尊重 `prefers-reduced-motion`（规则在 index.css）。
 */
export function PageTransition({ children }: { children: ReactNode }) {
  const { lastAction } = useLogicalNavigation();
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const action = lastAction();

  // 同一逻辑页面内的切换（tab、筛选器）不做动画。
  const direction = action === "replace" ? "none" : action === "back" ? "back" : "forward";

  return (
    <div className="page-transition" data-motion={direction} key={pathname}>
      {children}
    </div>
  );
}
