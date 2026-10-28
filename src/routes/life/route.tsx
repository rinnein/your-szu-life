import { createFileRoute, Outlet } from "@tanstack/react-router";

/**
 * `/life` 这一层的布局路由。
 *
 * 生活页本身是 `/life` 的 index 页面；子页面（`/life/campus-network`、
 * `/life/campus-payments/*`）直接挂在下一层，因此这里只做透传。
 */
export const Route = createFileRoute("/life")({
  component: Outlet,
});
