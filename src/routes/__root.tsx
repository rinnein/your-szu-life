import { LogicalNavigationProvider } from "../logical-navigation";
import { AndroidBackNavigation } from "../AndroidBackNavigation";
import { createRootRoute, HeadContent, Outlet } from "@tanstack/react-router";
import { Toast } from "@heroui/react";
import { AuthProvider } from "../auth";
import { AppContentProvider } from "../app-content/context";
import { UpdateNotice } from "../app-content/UpdateNotice";
import { ElectricityMonitorProvider } from "../electricity-monitor";
import { OjCompanionBridge } from "../oj/OjCompanionBridge";
import { OjProgressBridge } from "../oj/OjProgressBridge";
import { OjNotifications } from "../oj/OjNotifications";
import { QuizNotifications } from "../uooc/QuizNotifications";

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
    <LogicalNavigationProvider>
      <HeadContent />
      <Toast.Provider />
      <AppContentProvider>
        <ElectricityMonitorProvider>
          <AuthProvider>
            <AndroidBackNavigation />
            <QuizNotifications />
            <OjNotifications />
            <OjProgressBridge />
            <OjCompanionBridge />
            <Outlet />
            <UpdateNotice />
          </AuthProvider>
        </ElectricityMonitorProvider>
      </AppContentProvider>
    </LogicalNavigationProvider>
  ),
});
