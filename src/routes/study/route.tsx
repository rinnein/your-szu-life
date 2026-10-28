import { createFileRoute, Outlet } from "@tanstack/react-router";

/**
 * `/study` 这一层的布局路由。
 *
 * 学习页本身是 `/study` 的 index 页面；子页面（`/study/uooc` 等）直接挂在
 * 这一层下，因此这里只做透传，不渲染任何页面内容。
 */
export const Route = createFileRoute("/study")({
  component: Outlet,
});
