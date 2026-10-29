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
import {
  clearScrollCache,
  LogicalNavigationProvider,
  useLogicalNavigation,
} from "../logical-navigation";
import { AndroidBackNavigation } from "../AndroidBackNavigation";
import type { AndroidBackBridge } from "../android-back";
import StudyPage from "../StudyPage";
import OjPage from "./OjPage";
const native = vi.hoisted(() => ({
  close: undefined as undefined | ((event: { preventDefault: () => void }) => void),
  destroy: vi.fn(),
}));
vi.mock("../auth", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../auth")>();
  return {
    ...actual,
    useAuth: () => ({ state: { Authenticated: { studentId: "123456", name: "测试同学" } } }),
  };
});
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
function mount(entries = ["/", "/study", "/study/oj"]) {
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
      path: "/study",
      component: StudyPage,
    }),
    createRoute({
      getParentRoute: () => root,
      path: "/study/oj",
      validateSearch: () => ({ tab: "statement" }),
      component: OjPage,
    }),
  ]);
  const history = createMemoryHistory({ initialEntries: entries });
  const router = createRouter({ routeTree: tree, history, scrollRestoration: () => false });
  render(<RouterProvider router={router} />);
  return history;
}
beforeEach(() => {
  clearScrollCache();
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
it("uses standard icon controls to pop OJ to study", async () => {
  const history = mount();
  const back = await screen.findByRole("button", { name: "返回上一页" });
  // M3 顶栏的返回按钮是原生 button；data-webview-navigation 让 Android
  // 原生工具栏挂载时隐藏它，行为契约与迁移前一致。
  expect(back).toHaveAttribute("data-webview-navigation");
  expect(back.querySelector("svg")).toBeInTheDocument();
  fireEvent.click(back);
  await screen.findByRole("heading", { name: "学习" });
  expect(history.location.pathname).toBe("/study");
});
it("uses the same stack for desktop window close", async () => {
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  const history = mount();
  await screen.findByRole("button", { name: "返回上一页" });
  await waitFor(() => expect(native.close).toBeTypeOf("function"));
  const preventDefault = vi.fn();
  await act(async () => native.close!({ preventDefault }));
  await waitFor(() => expect(history.location.pathname).toBe("/study"));
  expect(preventDefault).toHaveBeenCalledOnce();
});
it("routes page presses through Android's bridge and hands off navigation after toolbar acknowledgement", async () => {
  const bridge: AndroidBackBridge = { postMessage: vi.fn(), onmessage: null };
  window.SzuApplicationBack = bridge;
  const history = mount();
  const button = await screen.findByRole("button", { name: "返回上一页" });
  expect(button).toHaveAttribute("data-webview-navigation");
  expect(document.documentElement).not.toHaveAttribute("data-szu-native-toolbar");
  fireEvent.click(button);
  expect(bridge.postMessage).toHaveBeenCalledWith("back-request");
  expect(history.location.pathname).toBe("/study/oj");
  await act(async () => bridge.onmessage?.({ data: "navigate-back" }));
  // 标题的可访问名称来自顶栏槽位里的标题文本，该节点带
  // data-webview-navigation：Android 原生工具栏挂载时隐藏它，避免重复标题。
  expect(await screen.findByRole("heading", { name: "学习" })).toBeInTheDocument();
  const title = document.querySelector(".top-app-bar-title");
  expect(title).toHaveAttribute("data-webview-navigation");
  expect(title).toHaveTextContent("学习");
  expect(document.documentElement).not.toHaveAttribute("data-szu-native-toolbar");
  act(() => bridge.onmessage?.({ data: "native-toolbar-ready" }));
  expect(document.documentElement).toHaveAttribute("data-szu-native-toolbar", "true");
});
it("leaves a child Activity's exit to the native bridge rather than WebView history", async () => {
  window.history.replaceState(null, "", "/study/oj?__szu_window=child");
  const bridge: AndroidBackBridge = { postMessage: vi.fn(), onmessage: null };
  window.SzuApplicationBack = bridge;
  const history = mount(["/study/oj"]);
  fireEvent.click(await screen.findByRole("button", { name: "返回上一页" }));
  expect(bridge.postMessage).toHaveBeenCalledWith("back-request");
  expect(history.canGoBack()).toBe(false);
  expect(native.destroy).not.toHaveBeenCalled();
});
it("从逻辑页面返回时恢复上一级逻辑页面本来所在的页面滚动状态", async () => {
  const scrollToSpy = vi.spyOn(window, "scrollTo").mockImplementation((...args: unknown[]) => {
    const options = args[0];
    if (
      typeof options === "object" &&
      options !== null &&
      "top" in options &&
      typeof options.top === "number"
    ) {
      window.scrollY = options.top;
    } else if (typeof options === "number") {
      window.scrollY = typeof args[1] === "number" ? args[1] : 0;
    }
  });

  const history = mount(["/", "/study"]);
  await screen.findByRole("heading", { name: "学习" });

  // 模拟在 /study 页面向下滚动到 750px
  window.scrollY = 750;

  // 点击学习页的 OJ 功能卡片进入子页面 /study/oj
  const ojCard = await screen.findByRole("button", { name: /OJ.*班级、测验与编程练习/ });
  fireEvent.click(ojCard);

  // 进入子逻辑页面后滚动应置顶为 0
  await screen.findByRole("button", { name: "返回上一页" });
  expect(history.location.pathname).toBe("/study/oj");
  expect(scrollToSpy).toHaveBeenCalledWith(expect.objectContaining({ top: 0 }));

  // 在子逻辑页面点击返回
  const back = await screen.findByRole("button", { name: "返回上一页" });
  fireEvent.click(back);

  // 返回上一级 /study 时，成功恢复原先的 750px 滚动高度
  await screen.findByRole("heading", { name: "学习" });
  expect(history.location.pathname).toBe("/study");
  expect(scrollToSpy).toHaveBeenCalledWith(expect.objectContaining({ top: 750 }));
  expect(window.scrollY).toBe(750);
});

it("在同一逻辑页面内使用 replace 切换 tab 时保持页面整体滚动高度不变", async () => {
  const scrollToSpy = vi.spyOn(window, "scrollTo").mockImplementation((...args: unknown[]) => {
    const options = args[0];
    if (
      typeof options === "object" &&
      options !== null &&
      "top" in options &&
      typeof options.top === "number"
    ) {
      window.scrollY = options.top;
    } else if (typeof options === "number") {
      window.scrollY = typeof args[1] === "number" ? args[1] : 0;
    }
  });

  let nav!: ReturnType<typeof useLogicalNavigation>;
  function TabPage() {
    nav = useLogicalNavigation();
    return <h1>Tab测试页</h1>;
  }

  const root = createRootRoute({
    component: () => (
      <LogicalNavigationProvider>
        <Outlet />
      </LogicalNavigationProvider>
    ),
  });
  const tree = root.addChildren([
    createRoute({ getParentRoute: () => root, path: "/test", component: TabPage }),
  ]);
  const history = createMemoryHistory({ initialEntries: ["/test?tab=a"] });
  const router = createRouter({ routeTree: tree, history, scrollRestoration: () => false });
  render(<RouterProvider router={router} />);

  await screen.findByRole("heading", { name: "Tab测试页" });

  // 模拟向下滚动到 500px
  window.scrollY = 500;

  // 切换 tab
  await act(async () => {
    await nav.replace("/test?tab=b");
  });

  // 断言未调用 scrollTo({ top: 0 })，且 window.scrollY 保持在 500
  expect(history.location.search).toBe("?tab=b");
  expect(scrollToSpy).not.toHaveBeenCalledWith(expect.objectContaining({ top: 0 }));
  expect(window.scrollY).toBe(500);
});
