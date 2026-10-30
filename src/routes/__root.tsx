import { LogicalNavigationProvider } from "../logical-navigation";
import { createRootRoute, HeadContent, Outlet } from "@tanstack/react-router";
import { AuthProvider } from "../auth";
import { AppBridges } from "../AppBridges";
import { AppContentProvider } from "../app-content/context";
import { UpdateNotice } from "../app-content/UpdateNotice";
import { CommandPalette } from "../command-palette";
import { ThemeProvider } from "../theme/ThemeProvider";
import { AppChrome } from "../shell/AppChrome";
import { OnboardingGate } from "../onboarding/OnboardingGate";

if (typeof window !== "undefined") {
  if (typeof window.matchMedia !== "function") {
    window.matchMedia = (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    });
  }
  if (typeof window.ResizeObserver !== "function") {
    window.ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    };
  }
}

/**
 * 根布局的职责划分：
 *
 * - 应用级基础设施（主题、逻辑导航、内容快照、根级桥接集合）常驻；
 * - 认证状态与初始化门一起包住真正可导航的应用内容；
 * - 快捷搜索与更新提示只属于应用内容，因此放在初始化门内部——
 *   认证页与初始化向导不会弹出它们；
 * - WebVPN 闸门只服务选课模块，电费状态只服务首页日历，分别在各自模块内挂载。
 */
export const Route = createRootRoute({
  head: () => ({ meta: [{ title: "泥深" }] }),
  component: () => (
    <ThemeProvider>
      <LogicalNavigationProvider>
        <HeadContent />
        <AppContentProvider>
          <AppBridges />
          <AppChrome>
            <AuthProvider>
              <OnboardingGate>
                <CommandPalette />
                <UpdateNotice />
                <Outlet />
              </OnboardingGate>
            </AuthProvider>
          </AppChrome>
        </AppContentProvider>
      </LogicalNavigationProvider>
    </ThemeProvider>
  ),
});
