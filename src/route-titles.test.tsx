/** @vitest-environment jsdom */
import { act, cleanup, render, waitFor } from "@testing-library/react";
import {
  createMemoryHistory,
  createRouter,
  HeadContent,
  Outlet,
  RouterProvider,
  type AnyRoute,
} from "@tanstack/react-router";
import { afterEach, expect, it } from "vite-plus/test";
import { routeTree } from "./routeTree.gen";

// Exercise the actual route metadata without starting authenticated services.
function presentationOnly(route: AnyRoute) {
  route.update({ component: Outlet });
  for (const child of Object.values(route.children ?? {})) presentationOnly(child as AnyRoute);
}
presentationOnly(routeTree);
routeTree.update({
  component: () => (
    <>
      <HeadContent />
      <Outlet />
    </>
  ),
});
afterEach(cleanup);

it("sets the document title from each route and updates it on tab replacement and back", async () => {
  const history = createMemoryHistory({ initialEntries: ["/"] });
  const router = createRouter({ routeTree, history });
  render(<RouterProvider router={router} />);
  await waitFor(() => expect(document.title).toBe("工作台"));
  for (const [to, title] of [
    ["/settings", "账户设置"],
    ["/class-timetable", "上课时间表"],
    ["/uooc", "UOOC刷课"],
    ["/oj", "OJ · 计软合集"],
    ["/computing", "计软合集"],
    ["/campus-network", "校园网"],
    ["/about", "关于泥深"],
    ["/debug", "诊断日志"],
    ["/campus-payments/card", "校园卡"],
    ["/campus-payments/network", "网费"],
    ["/campus-payments/electricity", "查电量"],
    ["/campus-payments/recharge", "充电费"],
  ]) {
    await act(async () => {
      await router.navigate({ to, replace: true });
    });
    await waitFor(() => expect(document.title).toBe(title));
    expect(document.head.querySelectorAll("title")).toHaveLength(1);
  }
  await act(async () => {
    await router.navigate({ to: "/about" });
  });
  await act(async () => {
    history.back();
  });
  await waitFor(() => expect(document.title).toBe("充电费"));
});

it("keeps numeric OJ deep-link IDs through router search parsing", async () => {
  const history = createMemoryHistory({
    initialEntries: ["/oj?site=szu-67&group=3&contest=12&problem=0%3A1&tab=answer"],
  });
  const router = createRouter({ routeTree, history });
  render(<RouterProvider router={router} />);
  await waitFor(() => expect(router.state.location.pathname).toBe("/oj"));
  await router.load();
  expect(router.state.matches[router.state.matches.length - 1]?.search).toMatchObject({
    site: "szu-67",
    group: "3",
    contest: "12",
    problem: "0:1",
    tab: "answer",
  });
});
