import type { NavigateOptions } from "@tanstack/react-router";

export interface Destination {
  value: string;
  label: string;
  /** Material Symbols 名称，material 的 icon 属性接受 SVG 标记。 */
  icon: string;
  to: NavigateOptions["to"];
  /** 仅桌面侧边导航展示。 */
  desktopOnly?: boolean;
}

/**
 * 全局导航目标。
 *
 * 移动端只显示 4 个（M3 NavigationBar 上限 5 个），
 * 桌面端侧边导航额外给出 UOOC / OJ / 校园卡等入口。
 */
export const DESTINATIONS: readonly Destination[] = [
  { value: "workbench", label: "工作台", icon: "home", to: "/" },
  { value: "enrollment", label: "选课", icon: "school", to: "/enrollment" },
  { value: "campus-network", label: "校园网", icon: "wifi", to: "/campus-network" },
  { value: "settings", label: "设置", icon: "settings", to: "/settings" },
  { value: "uooc", label: "UOOC", icon: "play_circle", to: "/uooc", desktopOnly: true },
  { value: "computing", label: "计软合集", icon: "code", to: "/computing", desktopOnly: true },
  {
    value: "campus-payments",
    label: "校园卡",
    icon: "wallet",
    to: "/campus-payments",
    desktopOnly: true,
  },
  { value: "about", label: "关于", icon: "info", to: "/about", desktopOnly: true },
] as const;

/** 当前路径落在哪个导航目标上。子页面归属其父模块。 */
export function activeDestination(pathname: string): string {
  if (pathname === "/") return "workbench";
  const match = DESTINATIONS.filter((item) => item.to !== "/")
    .filter((item) => pathname === item.to || pathname.startsWith(`${item.to}/`))
    // 最长前缀优先，避免 /campus-payments 抢走 /campus-payments/electricity 之外的情况。
    .sort((a, b) => String(b.to).length - String(a.to).length)[0];
  return match?.value ?? "workbench";
}
