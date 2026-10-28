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
import { icon } from "./icons";

/** 桌面侧边导航的断点。低于此宽度用底部导航栏。 */
const RAIL_QUERY = "(min-width: 900px)";

/**
 * 应用导航外壳。
 *
 * M3 规范：窄屏用底部 NavigationBar，宽屏用 NavigationRail；两者渲染
 * 完全相同的四个部件（首页 / 学习 / 生活 / 设置），子页面只影响高亮归属。
 * 导航走逻辑栈：顶级部件之间属于同级，因此用 `replace`，不堆积返回层级。
 */
export function AppNavigation() {
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const navigation = useLogicalNavigation();
  const wide = useMediaQuery(RAIL_QUERY);
  const active = activeDestination(pathname);

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
    <NavigationBar
      aria-label="主导航"
      value={active}
      onChange={(event) => navigate(event.detail.value)}
      className="app-nav-bar"
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
  );
}
