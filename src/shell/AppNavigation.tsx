import { useRouterState } from "@tanstack/react-router";
import {
  NavigationBar,
  NavigationBarItem,
  NavigationRail,
  NavigationRailItem,
} from "material/react";
import { useLogicalNavigation } from "../logical-navigation";
import { useMediaQuery } from "../responsive";
import { activeDestination, DESTINATIONS } from "./destinations";
import { icon, type IconName } from "./icons";

/** 桌面侧边导航的断点。低于此宽度用底部导航栏。 */
const RAIL_QUERY = "(min-width: 900px)";

/**
 * 应用导航外壳。
 *
 * M3 规范：窄屏用底部 NavigationBar（最多 5 项），宽屏用 NavigationRail。
 * 导航走逻辑栈的 `push`（独立页面）或 `replace`（同级切换）：
 * 顶级模块之间属于同级，因此用 `replace`，不堆积返回层级。
 */
export function AppNavigation() {
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const navigation = useLogicalNavigation();
  const wide = useMediaQuery(RAIL_QUERY);
  const active = activeDestination(pathname);

  const items = DESTINATIONS.filter((item) => wide || !item.desktopOnly);

  const navigate = (value: string) => {
    const destination = DESTINATIONS.find((item) => item.value === value);
    if (!destination) return;
    // 顶级模块之间是同级关系，替换而非压栈，返回键仍回到工作台。
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
        {items.map((item) => (
          <NavigationRailItem
            key={item.value}
            value={item.value}
            // 与页面内的同名入口（如工作台上的「选课」卡片）区分开，
            // 否则读屏与查询都会出现两个同名按钮。
            label={`导航 · ${item.label}`}
            icon={icon(item.icon as IconName)}
          />
        ))}
      </NavigationRail>
    );
  }

  return (
    <NavigationBar
      aria-label="主导航"
      value={active}
      onChange={(event) => navigate(event.detail.value)}
      className="app-nav-bar"
    >
      {items.map((item) => (
        <NavigationBarItem
          key={item.value}
          value={item.value}
          label={`导航 · ${item.label}`}
          icon={icon(item.icon as IconName)}
        />
      ))}
    </NavigationBar>
  );
}
