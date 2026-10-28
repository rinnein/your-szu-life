import { useEffect } from "react";
import { useRouterState } from "@tanstack/react-router";
import { postNativeMessage } from "../android-back";

/** 与原生约定的消息前缀；原生侧只接受本地主框架来源。 */
const STATUS_BAR_MESSAGE = "chrome-status-bar:";
const NAVIGATION_BAR_MESSAGE = "chrome-nav-bar:";

/**
 * material 顶栏把真实外壳放在 shadow root 里，颜色在那个元素上。
 * 这个选择器是在 shadow root **内部**查询的，不能再带宿主标签。
 */
const TOP_APP_BAR_SHELL = ".mtrl-top-app-bar";

/**
 * `rgb()` / `rgba()` 转 `#rrggbb`。
 *
 * `getComputedStyle` 只会给出 `rgb(...)`，而原生侧的颜色解析只认 3/6 位十六进制，
 * 因此在 Web 侧统一转换。
 */
export function rgbToHex(value: string | null | undefined): string | undefined {
  const match = /^rgba?\(\s*(\d+)[,\s]+(\d+)[,\s]+(\d+)(?:[,\s/]+([\d.]+)%?)?/i.exec(
    (value ?? "").trim(),
  );
  if (!match) return undefined;
  const channels = match.slice(1, 4).map(Number);
  if (channels.some((channel) => !Number.isFinite(channel) || channel > 255)) return undefined;
  // 完全透明（`transparent` 的 computed 值）不是一条可用的栏色，交给调用方走兜底。
  if (match[4] !== undefined && Number(match[4]) === 0) return undefined;
  return "#" + channels.map((channel) => channel.toString(16).padStart(2, "0")).join("");
}

function computedBackground(element: Element | null | undefined): string | undefined {
  if (!element || typeof getComputedStyle !== "function") return undefined;
  return rgbToHex(getComputedStyle(element).backgroundColor);
}

/** 主题角色色，作为没有对应控件时的兜底（如认证流程没有顶栏）。 */
function tokenColor(name: string): string | undefined {
  if (typeof getComputedStyle !== "function") return undefined;
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return /^#[0-9a-f]{6}$/i.test(value) ? value.toLowerCase() : undefined;
}

export type NativeChromeColors = { statusBar?: string; navigationBar?: string };

/**
 * 读取沉浸式系统栏颜色（Web 侧的真实来源，不在前端重复 M3 令牌值）。
 *
 * - 状态栏取顶部应用栏**当前**的颜色：material 的顶栏在页面滚动后会把它从
 *   `surface` 换成 `surface-container`，读 computed style 因此始终与它一致；
 *   没有顶栏的页面（统一认证流程）退回 `surface`。
 * - 小白条固定取底部导航栏的颜色（宽屏没有底部导航栏时同样退回 `surface-container`）。
 */
export function readNativeChromeColors(): NativeChromeColors {
  const shell = document
    .querySelector("m-top-app-bar")
    ?.shadowRoot?.querySelector(TOP_APP_BAR_SHELL);
  return {
    statusBar: computedBackground(shell) ?? tokenColor("--mtrl-sys-color-surface"),
    navigationBar:
      computedBackground(document.querySelector(".app-nav-bar")) ??
      tokenColor("--mtrl-sys-color-surface-container"),
  };
}

let lastPushed: NativeChromeColors = {};

/** 清空「上次推送值」缓存。给测试用，避免用例之间互相影响。 */
export function resetNativeChromeColors() {
  lastPushed = {};
}

/**
 * 把读到的颜色推给原生，但只在真的变了时才发消息。
 *
 * 滚动的每一帧都发会刷爆原生侧（历史上出现过把原生死循环压死的教训），因此这里按值去重。
 * 读不到颜色（非浏览器环境）时什么都不发。
 */
export function pushNativeChromeColors(colors: NativeChromeColors = readNativeChromeColors()) {
  const statusBar = colors.statusBar;
  if (statusBar && statusBar !== lastPushed.statusBar) {
    lastPushed = { ...lastPushed, statusBar };
    postNativeMessage(STATUS_BAR_MESSAGE + statusBar);
  }
  const navigationBar = colors.navigationBar;
  if (navigationBar && navigationBar !== lastPushed.navigationBar) {
    lastPushed = { ...lastPushed, navigationBar };
    postNativeMessage(NAVIGATION_BAR_MESSAGE + navigationBar);
  }
}

/**
 * 把系统栏颜色推给原生（桌面端没有该桥，静默跳过）。
 *
 * 触发点：挂载、路由切换（顶栏有无会变）、页面滚动（顶栏底色会变）、主题变化。
 */
export function useNativeChromeColors() {
  const pathname = useRouterState({ select: (state) => state.location.pathname });

  useEffect(() => {
    pushNativeChromeColors();
    let frame = 0;
    const onScroll = () => {
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        pushNativeChromeColors();
      });
    };
    window.addEventListener("scroll", onScroll, { passive: true });
    // 主题（种子色/深浅）换成另一套令牌后，两条系统栏的颜色也要跟着换。
    const observer = new MutationObserver(() => pushNativeChromeColors());
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme", "data-theme-mode"],
    });
    return () => {
      window.removeEventListener("scroll", onScroll);
      if (frame) cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, [pathname]);
}
