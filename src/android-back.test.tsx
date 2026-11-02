/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { StrictMode, useState } from "react";
import {
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  RouterProvider,
} from "@tanstack/react-router";
import { AndroidBackNavigation } from "./AndroidBackNavigation";
import { LogicalNavigationProvider } from "./logical-navigation";
import { BottomSheet, Button, Dialog, Drawer } from "material/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { activeOverlay, installAndroidBack, type AndroidBackBridge } from "./android-back";

let bridge: AndroidBackBridge;
let dispose = () => {};
let path = "/";
let update = () => {};
let goHome = vi.fn<() => Promise<void>>();
function install() {
  dispose = installAndroidBack({
    canBack: () => path !== "/",
    goBack: goHome,
    subscribe: (listener) => {
      update = listener;
      return vi.fn();
    },
  });
}
async function back() {
  await act(async () => {
    bridge.onmessage?.({ data: activeOverlay() ? "dismiss-overlay" : "navigate-back" });
  });
}
beforeEach(() => {
  path = "/";
  bridge = { postMessage: vi.fn(), onmessage: null };
  window.SzuApplicationBack = bridge;
  goHome = vi.fn(async () => {
    path = "/";
    update();
  });
});
afterEach(() => {
  dispose();
  cleanup();
  document.body.innerHTML = "";
  delete window.SzuApplicationBack;
});

describe("Android committed back", () => {
  it("hides web navigation only after the native toolbar confirms mounting", () => {
    install();
    expect(bridge.postMessage).toHaveBeenCalledWith("toolbar-query");
    expect(document.documentElement).not.toHaveAttribute("data-szu-native-toolbar");
    bridge.onmessage?.({ data: "native-toolbar-ready" });
    expect(document.documentElement).toHaveAttribute("data-szu-native-toolbar", "true");
    dispose();
    expect(document.documentElement).not.toHaveAttribute("data-szu-native-toolbar");
  });

  it("publishes a logical parent independently of overlay handling", () => {
    document.body.innerHTML = '<div role="dialog"></div>';
    install();
    expect(bridge.postMessage).toHaveBeenCalledWith("parent:no");
    expect(bridge.postMessage).toHaveBeenLastCalledWith("overlay");
    path = "/settings";
    update();
    expect(bridge.postMessage).toHaveBeenCalledWith("parent:yes");
    expect(bridge.postMessage).toHaveBeenLastCalledWith("overlay");
  });
  it("does nothing outside Android", () => {
    delete window.SzuApplicationBack;
    install();
    expect(bridge.postMessage).not.toHaveBeenCalled();
  });
  it("yields home to the system regardless of browser history", async () => {
    window.history.pushState({}, "", "/stale-webview-entry");
    install();
    expect(bridge.postMessage).toHaveBeenLastCalledWith("system");
    await back();
    expect(goHome).not.toHaveBeenCalled();
  });
  it.each([
    "/settings",
    "/study",
    "/life",
    "/study/class-timetable",
    "/life/campus-network",
    "/life/campus-payments/electricity",
    "/life/campus-payments/network",
  ])("returns %s to the home page rather than replaying browser history", async (route) => {
    path = route;
    install();
    expect(bridge.postMessage).toHaveBeenLastCalledWith("route");
    await back();
    expect(goHome).toHaveBeenCalledOnce();
    expect(bridge.postMessage).toHaveBeenLastCalledWith("system");
  });
  it("publishes route changes before another gesture", () => {
    install();
    path = "/settings";
    update();
    expect(bridge.postMessage).toHaveBeenLastCalledWith("route");
  });
  it("does not navigate on unknown/progress/cancel messages", async () => {
    path = "/study/uooc";
    install();
    for (const data of ["start", "progress", "cancel"]) bridge.onmessage?.({ data });
    expect(goHome).not.toHaveBeenCalled();
  });
  it("coalesces back while navigation is pending and recovers after failure", async () => {
    path = "/settings";
    let reject!: (error: Error) => void;
    goHome = vi.fn(
      () =>
        new Promise((_, fail) => {
          reject = fail;
        }),
    );
    install();
    await back();
    await back();
    expect(goHome).toHaveBeenCalledOnce();
    await act(async () => reject(new Error("blocked")));
    expect(bridge.postMessage).toHaveBeenLastCalledWith("route");
    await back();
    expect(goHome).toHaveBeenCalledTimes(2);
  });
  it("removes the listener and observer on cleanup", async () => {
    install();
    dispose();
    expect(bridge.onmessage).toBeNull();
    vi.mocked(bridge.postMessage).mockClear();
    document.body.innerHTML = '<div role="dialog"></div>';
    await Promise.resolve();
    expect(bridge.postMessage).not.toHaveBeenCalled();
  });
  it("ignores inline listboxes and hidden/exiting dialogs", () => {
    document.body.innerHTML =
      '<div role="listbox"></div><div hidden><div role="dialog"></div></div><div role="dialog" data-exiting></div>';
    expect(activeOverlay()).toBeUndefined();
  });
  it("consumes back without navigating when an overlay declines Escape", async () => {
    path = "/settings";
    document.body.innerHTML = '<div role="alertdialog" tabindex="-1"></div>';
    install();
    await back();
    expect(goHome).not.toHaveBeenCalled();
    expect(bridge.postMessage).toHaveBeenLastCalledWith("overlay");
  });
  it("closes a real material dialog without navigating", async () => {
    function Example() {
      const [open, setOpen] = useState(true);
      if (!open) return null;
      return (
        <Dialog open={open} headline="测试对话框" onClose={() => setOpen(false)}>
          <div>
            <Button>Confirm</Button>
          </div>
        </Dialog>
      );
    }
    render(<Example />);
    install();
    expect(activeOverlay()).toBeDefined();
    expect(bridge.postMessage).toHaveBeenLastCalledWith("overlay");
    await back();
    await waitFor(() => expect(activeOverlay()).toBeUndefined());
    await waitFor(() => expect(bridge.postMessage).toHaveBeenLastCalledWith("system"));
    expect(goHome).not.toHaveBeenCalled();
  });
  it("republishes overlay state when an already mounted dialog opens", async () => {
    function Example() {
      const [open, setOpen] = useState(false);
      return (
        <>
          <button type="button" onClick={() => setOpen(true)}>
            注销
          </button>
          <Dialog open={open} headline="确认注销？" onClose={() => setOpen(false)}>
            <p>正文</p>
          </Dialog>
        </>
      );
    }
    render(<Example />);
    install();
    expect(bridge.postMessage).toHaveBeenLastCalledWith("system");
    fireEvent.click(screen.getByRole("button", { name: "注销" }));
    await waitFor(() => expect(bridge.postMessage).toHaveBeenLastCalledWith("overlay"));
    expect(activeOverlay()).toBeDefined();
    await back();
    await waitFor(() => expect(activeOverlay()).toBeUndefined());
    expect(goHome).not.toHaveBeenCalled();
    await waitFor(() => expect(bridge.postMessage).toHaveBeenLastCalledWith("system"));
  });
  it("closes a modal drawer instead of navigating", async () => {
    function Example() {
      const [open, setOpen] = useState(true);
      return (
        <Drawer modal open={open} headline="页面导航" onClose={() => setOpen(false)}>
          <span>条件</span>
        </Drawer>
      );
    }
    render(<Example />);
    install();
    expect(activeOverlay()).toBeDefined();
    expect(bridge.postMessage).toHaveBeenLastCalledWith("overlay");
    await back();
    await waitFor(() => expect(activeOverlay()).toBeUndefined());
    expect(goHome).not.toHaveBeenCalled();
  });
  it("does not treat a docked drawer as a modal overlay", async () => {
    render(
      <Drawer open headline="题目">
        <span>题目列表</span>
      </Drawer>,
    );
    install();
    expect(activeOverlay()).toBeUndefined();
    expect(bridge.postMessage).toHaveBeenLastCalledWith("system");
    path = "/settings";
    update();
    await back();
    expect(goHome).toHaveBeenCalledOnce();
  });
  it("closes a modal bottom sheet instead of navigating", async () => {
    function Example() {
      const [open, setOpen] = useState(true);
      return (
        <BottomSheet modal open={open} headline="快捷搜索" onClose={() => setOpen(false)}>
          <span>内容</span>
        </BottomSheet>
      );
    }
    render(<Example />);
    install();
    expect(activeOverlay()).toBeDefined();
    await back();
    await waitFor(() => expect(activeOverlay()).toBeUndefined());
    expect(goHome).not.toHaveBeenCalled();
  });
  it("closes only the topmost modal", async () => {
    const outer = document.createElement("m-dialog");
    outer.setAttribute("open", "");
    const inner = document.createElement("m-dialog");
    inner.setAttribute("open", "");
    outer.appendChild(inner);
    document.body.appendChild(outer);
    install();
    expect(activeOverlay()).toBe(inner);
    await back();
    expect(inner.hasAttribute("open")).toBe(false);
    expect(outer.hasAttribute("open")).toBe(true);
    expect(goHome).not.toHaveBeenCalled();
  });
});

it("connects real TanStack route resolution and survives StrictMode setup/cleanup", async () => {
  const root = createRootRoute({
    component: () => (
      <LogicalNavigationProvider>
        <AndroidBackNavigation />
        <Outlet />
      </LogicalNavigationProvider>
    ),
  });
  const index = createRoute({
    getParentRoute: () => root,
    path: "/",
    component: () => <p>Workbench</p>,
  });
  const settings = createRoute({
    getParentRoute: () => root,
    path: "/settings",
    component: () => <p>Settings</p>,
  });
  const router = createRouter({
    routeTree: root.addChildren([index, settings]),
    history: createMemoryHistory({ initialEntries: ["/", "/settings"] }),
  });
  render(
    <StrictMode>
      <RouterProvider router={router} />
    </StrictMode>,
  );
  await waitFor(() => expect(bridge.postMessage).toHaveBeenLastCalledWith("route"));
  await back();
  await waitFor(() => expect(router.state.location.pathname).toBe("/"));
  await waitFor(() => expect(bridge.postMessage).toHaveBeenLastCalledWith("system"));
  await act(async () => {
    await router.navigate({ to: "/settings" });
  });
  await waitFor(() => expect(bridge.postMessage).toHaveBeenLastCalledWith("route"));
});

it("does not turn a stale overlay dismissal into a child-window route change", async () => {
  path = "/settings";
  install();
  await act(async () => {
    bridge.onmessage?.({ data: "dismiss-overlay" });
  });
  expect(goHome).not.toHaveBeenCalled();
  expect(bridge.postMessage).toHaveBeenLastCalledWith("route");
});

it("publishes the overlay separately from the non-root route beneath it", async () => {
  path = "/settings";
  install();
  expect(bridge.postMessage).toHaveBeenLastCalledWith("route");
  document.body.innerHTML = '<div role="dialog"></div>';
  await waitFor(() => expect(bridge.postMessage).toHaveBeenLastCalledWith("overlay"));
  document.body.innerHTML = "";
  await waitFor(() => expect(bridge.postMessage).toHaveBeenLastCalledWith("route"));
  dispose();
  expect(bridge.postMessage).toHaveBeenLastCalledWith("loading");
});
