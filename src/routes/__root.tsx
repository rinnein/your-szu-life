import { createRootRoute, Outlet } from "@tanstack/react-router";
import { AuthProvider } from "../auth";

export const Route = createRootRoute({
  component: () => (
    <AuthProvider>
      <Outlet />
    </AuthProvider>
  ),
});
