/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import {
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  RouterProvider,
} from "@tanstack/react-router";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { LogicalNavigationProvider } from "../logical-navigation";
import { AndroidBackNavigation } from "../AndroidBackNavigation";
import type { AndroidBackBridge } from "../android-back";
import ComputingCollectionPage from "../ComputingCollectionPage";
import OjPage from "./OjPage";
const native = vi.hoisted(() => ({
  close: undefined as undefined | ((event: { preventDefault: () => void }) => void),
  destroy: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async () => ({ sites: [], notices: [], desktop: false, companionPort: 27121 })),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  WebviewWindow: vi.fn(),
  getCurrentWebviewWindow: () => ({
    destroy: native.destroy,
    onCloseRequested: async (callback: typeof native.close) => {
      native.close = callback;
      return () => {};
    },
  }),
}));
const originalUrl = window.location.href;
function mount(entries = ["/", "/computing", "/oj"]) {
  const root = createRootRoute({
    component: () => (
      <LogicalNavigationProvider>
        <AndroidBackNavigation />
        <Outlet />
      </LogicalNavigationProvider>
    ),
  });
  const tree = root.addChildren([
    createRoute({ getParentRoute: () => root, path: "/", component: () => <h1>工作台</h1> }),
    createRoute({
      getParentRoute: () => root,
      path: "/computing",
      component: ComputingCollectionPage,
    }),
    createRoute({
      getParentRoute: () => root,
      path: "/oj",
      validateSearch: () => ({ tab: "statement" }),
      component: OjPage,
    }),
  ]);
  const history = createMemoryHistory({ initialEntries: entries });
  const router = createRouter({ routeTree: tree, history });
  render(<RouterProvider router={router} />);
  return history;
}
beforeEach(() => {
  native.close = undefined;
  native.destroy.mockReset();
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
  vi.stubGlobal(
    "matchMedia",
    vi.fn().mockImplementation((media) => ({
      media,
      matches: false,
      addEventListener() {},
      removeEventListener() {},
      addListener() {},
      removeListener() {},
    })),
  );
});
afterEach(() => {
  cleanup();
  delete window.SzuApplicationBack;
  delete document.documentElement.dataset.szuNativeToolbar;
  window.history.replaceState(null, "", originalUrl);
  vi.unstubAllGlobals();
});
it("uses standard icon controls to pop OJ, collection and workbench in order", async () => {
  const history = mount();
  const back = await screen.findByRole("button", { name: "返回上一级" });
  // M3 顶栏的返回按钮是原生 button；data-webview-navigation 让 Android
  // 原生工具栏挂载时隐藏它，行为契约与迁移前一致。
  expect(back).toHaveAttribute("data-webview-navigation");
  expect(back.querySelector("svg")).toBeInTheDocument();
  fireEvent.click(back);
  await screen.findByRole("heading", { name: "计软合集" });
  expect(history.location.pathname).toBe("/computing");
  const parent = screen.getByRole("button", { name: "返回工作台" });
  expect(parent).toHaveAttribute("data-webview-navigation");
  fireEvent.click(parent);
  await screen.findByRole("heading", { name: "工作台" });
  expect(history.canGoBack()).toBe(false);
});
it("uses the same stack for desktop window close", async () => {
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  const history = mount();
  await screen.findByRole("button", { name: "返回上一级" });
  await waitFor(() => expect(native.close).toBeTypeOf("function"));
  const preventDefault = vi.fn();
  await act(async () => native.close!({ preventDefault }));
  await waitFor(() => expect(history.location.pathname).toBe("/computing"));
  expect(preventDefault).toHaveBeenCalledOnce();
});
it("routes page presses through Android's bridge and hands off navigation after toolbar acknowledgement", async () => {
  const bridge: AndroidBackBridge = { postMessage: vi.fn(), onmessage: null };
  window.SzuApplicationBack = bridge;
  const history = mount();
  const button = await screen.findByRole("button", { name: "返回上一级" });
  expect(button).toHaveAttribute("data-webview-navigation");
  expect(document.documentElement).not.toHaveAttribute("data-szu-native-toolbar");
  fireEvent.click(button);
  expect(bridge.postMessage).toHaveBeenCalledWith("back-request");
  expect(history.location.pathname).toBe("/oj");
  await act(async () => bridge.onmessage?.({ data: "navigate-back" }));
  const heading = await screen.findByRole("heading", { name: "计软合集" });
  expect(heading).toHaveAttribute("data-webview-navigation");
  expect(screen.getByRole("button", { name: "返回工作台" })).toHaveAttribute(
    "data-webview-navigation",
  );
  expect(document.documentElement).not.toHaveAttribute("data-szu-native-toolbar");
  act(() => bridge.onmessage?.({ data: "native-toolbar-ready" }));
  expect(document.documentElement).toHaveAttribute("data-szu-native-toolbar", "true");
});
it("leaves a child Activity's exit to the native bridge rather than WebView history", async () => {
  window.history.replaceState(null, "", "/oj?__szu_window=child");
  const bridge: AndroidBackBridge = { postMessage: vi.fn(), onmessage: null };
  window.SzuApplicationBack = bridge;
  const history = mount(["/oj"]);
  fireEvent.click(await screen.findByRole("button", { name: "返回上一级" }));
  expect(bridge.postMessage).toHaveBeenCalledWith("back-request");
  expect(history.canGoBack()).toBe(false);
  expect(native.destroy).not.toHaveBeenCalled();
});
