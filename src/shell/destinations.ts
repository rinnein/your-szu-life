import type { NavigateOptions } from "@tanstack/react-router";
import type { IconName } from "./icons";

export interface Destination {
  value: "home" | "study" | "life" | "settings";
  label: string;
  /** Material Symbols 名称；material 的 icon 属性接受 SVG 标记。 */
  icon: IconName;
  to: NavigateOptions["to"];
}

/**
 * 全局导航目标。
 *
 * 就这四个，宽屏侧边导航（rail）与窄屏底部导航栏（bar）完全一致。
 * 子页面在路径上带归属前缀（`/study/uooc`、`/life/campus-payments/electricity`），
 * 因此「当前落在哪个部件」只需要看路径第一段。
 */
export const DESTINATIONS: readonly Destination[] = [
  { value: "home", label: "首页", icon: "home", to: "/" },
  { value: "study", label: "学习", icon: "school", to: "/study" },
  { value: "life", label: "生活", icon: "wallet", to: "/life" },
  { value: "settings", label: "设置", icon: "settings", to: "/settings" },
] as const;

/** 当前路径落在哪个导航部件上。 */
export function activeDestination(pathname: string): Destination["value"] {
  const segment = pathname.split("/")[1] ?? "";
  const match = DESTINATIONS.find((item) => item.value !== "home" && item.value === segment);
  return match?.value ?? "home";
}
