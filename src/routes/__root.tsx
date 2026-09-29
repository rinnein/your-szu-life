import { LogicalNavigationProvider } from "../logical-navigation";
import { AndroidBackNavigation } from "../AndroidBackNavigation";
import { createRootRoute, HeadContent, Outlet } from "@tanstack/react-router";
import { AuthProvider } from "../auth";
import { AppContentProvider } from "../app-content/context";
import { UpdateNotice } from "../app-content/UpdateNotice";
import { ElectricityMonitorProvider } from "../electricity-monitor";

export const Route = createRootRoute({
  head: () => ({ meta: [{ title: "泥深" }] }),
  component: () => (
    <LogicalNavigationProvider>
      <HeadContent />
      <AppContentProvider>
        <ElectricityMonitorProvider>
          <AuthProvider>
            <AndroidBackNavigation />
            <Outlet />
            <UpdateNotice />
          </AuthProvider>
        </ElectricityMonitorProvider>
      </AppContentProvider>
    </LogicalNavigationProvider>
  ),
});
