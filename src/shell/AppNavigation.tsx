import { useEffect, useState } from "react";
import { useRouterState } from "@tanstack/react-router";
import { motion, useReducedMotion } from "motion/react";
import {
  NavigationBar,
  NavigationBarItem,
  NavigationRail,
  NavigationRailItem,
} from "material/react";
import { useLogicalNavigation } from "../logical-navigation";
import { useMediaQuery } from "../responsive";
import { navigationIndicator } from "../motion/tokens";
import { activeDestination, DESTINATIONS } from "./destinations";
import { icon } from "./icons";
import { useNavigationIndicatorBox } from "./use-navigation-indicator";

/** 桌面侧边导航的断点。低于此宽度用底部导航栏。 */
const RAIL_QUERY = "(min-width: 900px)";

/**
 * 应用导航外壳。
 *
 * M3 规范：窄屏用底部 NavigationBar，宽屏用 NavigationRail；两者渲染
 * 完全相同的四个部件（首页 / 学习 / 生活 / 设置），子页面只影响高亮归属。
 * 导航走逻辑栈：顶级部件之间属于同级，因此用 `replace`，不堆积返回层级。
 *
 * 窄屏底栏的选中药丸由外层 `.app-nav-bar` 里的 motion 浮层绘制，跨 item
 * 滑动；material 宿主退成透明的 `.app-nav-bar-inner`（它自带的每个 item
 * 独立淡入的药丸被令牌覆盖隐藏，仍负责点击、键盘与 `aria-current`）。
 * 宽屏 rail 保留 material 自带的指示器。
 */
export function AppNavigation() {
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const navigation = useLogicalNavigation();
  const wide = useMediaQuery(RAIL_QUERY);
  const active = activeDestination(pathname);
  const reduceMotion = useReducedMotion();
  // 用 state 持有节点（回调 ref）：测量只在窄屏分支需要，节点一变就重新挂观测器。
  const [shell, setShell] = useState<HTMLDivElement | null>(null);
  const [bar, setBar] = useState<HTMLElement | null>(null);
  const indicatorBox = useNavigationIndicatorBox(bar, shell, active);

  // 动态测量底部导航栏高度，供弹窗（如 Snackbar）避让定位使用
  useEffect(() => {
    if (typeof document === "undefined") return;
    if (wide) {
      document.documentElement.style.removeProperty("--app-nav-bar-height");
      return;
    }
    const update = () => {
      const bar = document.querySelector(".app-nav-bar");
      if (!bar) {
        document.documentElement.style.removeProperty("--app-nav-bar-height");
        return;
      }
      const rect = bar.getBoundingClientRect();
      if (rect.height > 0) {
        document.documentElement.style.setProperty("--app-nav-bar-height", `${rect.height}px`);
      } else {
        document.documentElement.style.removeProperty("--app-nav-bar-height");
      }
    };
    update();
    const bar = document.querySelector(".app-nav-bar");
    const ro = bar && typeof ResizeObserver !== "undefined" ? new ResizeObserver(update) : null;
    if (bar && ro) ro.observe(bar);
    return () => {
      ro?.disconnect();
      document.documentElement.style.removeProperty("--app-nav-bar-height");
    };
  }, [wide, pathname]);

  const navigate = (value: string) => {
    const destination = DESTINATIONS.find((item) => item.value === value);
    if (!destination) return;
    // 顶级部件之间是同级关系，替换而非压栈，返回键仍回到首页。
    void navigation.replace(String(destination.to));
  };

  if (wide) {
    return (
      <NavigationRail
        aria-label="主导航"
        value={active}
        onChange={(event) => navigate(event.detail.value)}
        className="app-rail"
      >
        {DESTINATIONS.map((item) => (
          <NavigationRailItem
            key={item.value}
            value={item.value}
            label={item.label}
            icon={icon(item.icon)}
          />
        ))}
      </NavigationRail>
    );
  }

  return (
    <div ref={setShell} className="app-nav-bar">
      {indicatorBox && (
        <motion.div
          aria-hidden="true"
          className="app-nav-indicator"
          // 只给显式属性打补丁：给绝对定位元素做 x/y 位移会盖掉 CSS 里的
          // `inset-inline`（宽屏切窄屏时药丸会留在原来的偏移上）。
          // 禁用动效时直接跳到目标位置，不做过渡。
          initial={false}
          style={{ left: indicatorBox.left, top: indicatorBox.top }}
          animate={{ width: indicatorBox.width, height: indicatorBox.height }}
          transition={reduceMotion ? { duration: 0 } : navigationIndicator}
        />
      )}
      <NavigationBar
        ref={setBar}
        aria-label="主导航"
        value={active}
        onChange={(event) => navigate(event.detail.value)}
        className="app-nav-bar-inner"
      >
        {DESTINATIONS.map((item) => (
          <NavigationBarItem
            key={item.value}
            value={item.value}
            label={item.label}
            icon={icon(item.icon)}
          />
        ))}
      </NavigationBar>
    </div>
  );
}
