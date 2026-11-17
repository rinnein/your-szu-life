import { useRouterState } from "@tanstack/react-router";
import { type ReactNode, useLayoutEffect, useRef } from "react";
import { animate, useReducedMotion } from "motion/react";
import { useLogicalNavigation } from "../logical-navigation";
import { activeDestination, type Destination } from "../shell/destinations";
import { pageTransitionKind, type PageTransitionKind } from "./page-transition-kind";
import { fadeThrough } from "./tokens";

/**
 * 页面过渡。
 *
 * 三种入场：`push`/首次挂载是建立页面层级，向前入场；`back` 是回退层级，
 * 向后入场 —— 这两者用 index.css 里的 CSS 动画；顶级部件之间切换（底栏/
 * rail，路径第一段变化）是同级替换，用 M3 的 fade-through（motion）。
 * 同一逻辑页面内的 tab、筛选器切换是 `replace`，不播放任何动画，否则每次
 * 切 tab 都会整页淡入，观感与 Android 原生不符。
 *
 * 不靠给包裹元素换 `key` 来重播动画：换 key 会在同一批次里卸载整棵路由
 * 子树，打断正在进行的 history 回退（表现为点返回后停在原页，见
 * src/logical-navigation.tsx 的 `back`）。CSS 动画在同一次动画上做一次
 * 强制回流把它从头播一遍，React 的挂载/卸载时机完全不参与；fade-through
 * 用 `animate()` 直接驱动这个元素，每次重跑 effect 都会先停掉上一段。
 *
 * 尊重 `prefers-reduced-motion`。
 */
export function PageTransition({ children }: { children: ReactNode }) {
  const { lastAction } = useLogicalNavigation();
  // 用完整 href 而不是 pathname：设置页这类同一路径下只换 query 的
  // `replace` 也要被观察到（虽然它不播放动画）。
  const href = useRouterState({ select: (state) => state.location.href });
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const destination = activeDestination(pathname);
  const reduceMotion = useReducedMotion();
  const ref = useRef<HTMLDivElement>(null);
  /** 上一次渲染落在哪个顶级部件；`undefined` 表示还没记录过（首次挂载）。 */
  const previousDestination = useRef<Destination["value"] | undefined>(undefined);
  /**
   * 上一次播放的 `{ href, kind }`。
   *
   * 同一次导航里 effect 可能重跑（StrictMode 的开发期双调用），此时上一次
   * 记录的部件已经改成本次的目标部件，再算一次就会得到 `none` 并停掉刚
   * 开始的动画。键相同的重跑直接复用上次的结果，只有真正的导航才重新判定。
   */
  const lastPlayed = useRef<{ href: string; kind: PageTransitionKind } | null>(null);

  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;

    let kind: PageTransitionKind;
    if (lastPlayed.current?.href === href) {
      kind = lastPlayed.current.kind;
    } else {
      kind = pageTransitionKind(lastAction(), previousDestination.current ?? null, destination);
      lastPlayed.current = { href, kind };
      previousDestination.current = destination;
    }

    // 先复位：CSS 动画与 motion 驱动的是同一个元素，切换入场方式前必须
    // 清掉上一种的内联样式与 `data-motion`。
    element.dataset.motion = "none";
    element.style.opacity = "";
    element.style.transform = "";
    if (kind === "none" || reduceMotion) return;

    if (kind === "fade-through") {
      // 只动 opacity 与 transform，不改变文档高度：滚动位置在导航解析时
      // 已被 LogicalNavigationProvider 复位。
      const controls = animate(
        element,
        { opacity: [0, 1], transform: [`scale(${fadeThrough.scale})`, "scale(1)"] },
        { duration: fadeThrough.duration, ease: fadeThrough.ease },
      );
      return () => controls.stop();
    }

    // 先复位再设回，让同一个 CSS 动画从头播一次。
    void element.offsetWidth;
    element.dataset.motion = kind;
  }, [href, destination, lastAction, reduceMotion]);

  return (
    <div ref={ref} className="page-transition">
      {children}
    </div>
  );
}
