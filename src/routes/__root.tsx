import { createRootRoute, Outlet } from "@tanstack/react-router";
import { AuthProvider } from "../auth";
import { AppContentProvider } from "../app-content/context";
import { UpdateNotice } from "../app-content/UpdateNotice";

export const Route = createRootRoute({
  component: () => (
    <AppContentProvider>
      <AuthProvider>
        <Outlet />
        <UpdateNotice />
      </AuthProvider>
    </AppContentProvider>
  ),
});
