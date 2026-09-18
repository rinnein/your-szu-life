import { createRootRoute, Outlet } from "@tanstack/react-router";
import { AuthProvider } from "../auth";
import { AppContentProvider } from "../app-content/context";
import { UpdateNotice } from "../app-content/UpdateNotice";
import { ElectricityMonitorProvider } from "../electricity-monitor";

export const Route = createRootRoute({
  component: () => (
    <AppContentProvider>
      <ElectricityMonitorProvider>
        <AuthProvider>
          <Outlet />
          <UpdateNotice />
        </AuthProvider>
      </ElectricityMonitorProvider>
    </AppContentProvider>
  ),
});
