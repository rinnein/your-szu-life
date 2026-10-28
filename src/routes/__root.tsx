import { LogicalNavigationProvider } from "../logical-navigation";
import { AndroidBackNavigation } from "../AndroidBackNavigation";
import { DeepLinkNavigation } from "../DeepLinkNavigation";
import { createRootRoute, HeadContent, Outlet } from "@tanstack/react-router";
import { AuthProvider } from "../auth";
import { AppContentProvider } from "../app-content/context";
import { UpdateNotice } from "../app-content/UpdateNotice";
import { ElectricityMonitorProvider } from "../electricity-monitor";
import { OjCompanionBridge } from "../oj/OjCompanionBridge";
import { OjProgressBridge } from "../oj/OjProgressBridge";
import { OjNotifications } from "../oj/OjNotifications";
import { QuizNotifications } from "../uooc/QuizNotifications";
import { InAppNotificationBridge } from "../InAppNotificationBridge";
import { CommandPalette } from "../command-palette";
import { ThemeProvider } from "../theme/ThemeProvider";
import { AppChrome } from "../shell/AppChrome";
import { WebvpnAuthProvider } from "../enrollment/webvpn-gate";

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

export const Route = createRootRoute({
  head: () => ({ meta: [{ title: "泥深" }] }),
  component: () => (
    <ThemeProvider>
      <LogicalNavigationProvider>
        <HeadContent />
        <AppContentProvider>
          <ElectricityMonitorProvider>
            <AuthProvider>
              <AndroidBackNavigation />
              <DeepLinkNavigation />
              <QuizNotifications />
              <OjNotifications />
              <InAppNotificationBridge />
              <OjProgressBridge />
              <OjCompanionBridge />
              <CommandPalette />
              <WebvpnAuthProvider>
                <AppChrome>
                  <Outlet />
                </AppChrome>
              </WebvpnAuthProvider>
              <UpdateNotice />
            </AuthProvider>
          </ElectricityMonitorProvider>
        </AppContentProvider>
      </LogicalNavigationProvider>
    </ThemeProvider>
  ),
});
