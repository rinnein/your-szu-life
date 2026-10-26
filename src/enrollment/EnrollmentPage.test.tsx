/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createMemoryHistory, createRouter, RouterProvider } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { routeTree } from "../routeTree.gen";
import { initialLogicalEntries } from "../logical-navigation";
import { snapshotFixture } from "./testing/fixtures";
import type { Snapshot } from "./model";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
Element.prototype.getAnimations = () => [];
let snapshot: Snapshot;
let auth = "ready";
function mount(path = "/enrollment") {
  const router = createRouter({
    routeTree,
    history: createMemoryHistory({
      initialEntries: initialLogicalEntries(`https://tauri.localhost${path}`),
    }),
  });
  render(<RouterProvider router={router} />);
  return router;
}
beforeEach(() => {
  vi.clearAllMocks();
  auth = "ready";
  snapshot = structuredClone(snapshotFixture);
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
  vi.spyOn(window, "scrollTo").mockImplementation(() => {});
  vi.mocked(listen).mockResolvedValue(() => {});
  vi.mocked(invoke).mockImplementation(async (name, args) => {
    if (name === "get_auth_state")
      return { Authenticated: { studentId: "synthetic", name: "演示同学" } };
    if (name !== "enrollment_command") return undefined;
    const a = (args as Record<string, unknown>).action as {
      type: string;
      mode?: "campus" | "webvpn";
    };
    if (a.type === "cacheSnapshot") return { ...snapshot, loggedIn: auth === "ready" };
    if (a.type === "cache")
      return {
        context: snapshot.context,
        courses: snapshot.courses,
        total: snapshot.courses.length,
        complete: true,
        updatedAt: snapshot.updatedAt,
        cachedPages: 1,
        query: (a as { query?: Snapshot["query"] }).query ?? snapshot.query,
        coverage: "partial",
      };
    if (a.type === "session")
      return { status: auth, snapshot: { ...snapshot, loggedIn: auth === "ready" } };
    if (a.type === "captchaPrepare") {
      auth = "ready";
      return { status: "authenticated", snapshot: { ...snapshot, loggedIn: true } };
    }
    if (a.type === "options") return { batches: [], campuses: [] };
    if (a.type === "probe")
      return [
        { mode: "campus", latency: 10, needs2fa: false, message: "" },
        { mode: "webvpn", latency: null, needs2fa: true, message: "需2FA验证" },
      ];
    if (a.type === "mode") snapshot = { ...snapshot, mode: a.mode!, loggedIn: false };
    return snapshot;
  });
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});
it("uses the existing arrow header and keeps authentication out of the catalog", async () => {
  mount();
  await screen.findByRole("heading", { name: "查找课程" });
  expect(await screen.findByRole("button", { name: "返回工作台" })).toHaveAttribute(
    "data-webview-navigation",
  );
  await screen.findByText("中国历史与文化");
  expect(screen.queryByRole("button", { name: "校内访问" })).toBeNull();
  expect(screen.queryByText("连接学校选课系统")).toBeNull();
});
it("opens functional child pages and replaces sibling navigation", async () => {
  const router = mount();
  await screen.findByText("中国历史与文化");
  const initial = router.history.length;
  fireEvent.click(screen.getByRole("button", { name: "抢课任务" }));
  await screen.findByRole("heading", { name: "抢课任务" });
  expect(router.history.length).toBe(initial + 1);
  await waitFor(() => expect(screen.queryByLabelText("搜索课程")).toBeNull());
  fireEvent.click(screen.getByRole("button", { name: "选课设置" }));
  await screen.findByText("新任务默认配置");
  expect(router.history.length).toBe(initial + 1);
  fireEvent.click(screen.getByRole("button", { name: "返回选课" }));
  await waitFor(() =>
    expect(router.state.location.pathname.replace(/\/$/, "")).toBe("/enrollment"),
  );
  expect(
    vi
      .mocked(invoke)
      .mock.calls.some(
        ([, args]) =>
          ((args as Record<string, unknown>)?.action as { type?: string })?.type === "control",
      ),
  ).toBe(false);
});
it("migrates legacy tab links without stacking an extra route", async () => {
  const router = mount("/enrollment?tab=tasks&keyword=CS101");
  await screen.findByRole("heading", { name: "抢课任务" });
  expect(router.state.location.pathname).toBe("/enrollment/tasks");
  expect(router.state.location.searchStr).toContain("CS101");
});
it("redirects an invalid session to dedicated login and replaces it after login", async () => {
  auth = "required";
  const router = mount();
  // 会话探测是异步的：布局在拿到结果前先渲染页面内容，随后才 replace 到登录页。
  await screen.findByRole("heading", { name: "登录选课系统" });
  fireEvent.click(await screen.findByRole("button", { name: "登录并进入选课" }));
  await screen.findByRole("heading", { name: "查找课程" });
  expect(router.state.location.pathname.replace(/\/$/, "")).toBe("/enrollment");
  fireEvent.click(screen.getByRole("button", { name: "返回工作台" }));
  await waitFor(() => expect(router.state.location.pathname).toBe("/"));
});
it("opens dedicated 2FA from the login access card", async () => {
  auth = "required";
  mount("/enrollment/login");
  await screen.findByText("需2FA验证");
  fireEvent.click(screen.getByRole("radio", { name: "WebVPN访问" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("open_academic_portal", { kind: "webvpn", target: null }),
  );
});
it("renders a separate timetable with a week control", async () => {
  mount("/enrollment/timetable");
  await screen.findByRole("grid", { name: "选课期间课表" });
  expect(screen.getAllByRole("columnheader")).toHaveLength(8);
  expect(screen.getByText("显示周次")).toBeInTheDocument();
  expect(screen.queryByText("搜索课程")).toBeNull();
});
it("pins without modifying the execution queue", async () => {
  mount();
  fireEvent.click(await screen.findByRole("button", { name: "置顶 中国历史与文化" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("enrollment_command", {
      action: { type: "pin", id: "class-new", pinned: true },
    }),
  );
});

it("shows conflicting academic credits instead of silently selecting one record", async () => {
  mount();
  await screen.findByText("中国历史与文化");
  expect(screen.getByText("1、4 学分")).toBeInTheDocument();
  expect(screen.getByText("学业记录有差异")).toBeInTheDocument();
});

it("shows cached courses while session verification is pending", async () => {
  const original = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((name, args) => {
    if (
      name === "enrollment_command" &&
      ((args as Record<string, unknown>).action as { type: string }).type === "session"
    )
      return new Promise(() => {});
    return original(name, args);
  });
  mount();
  expect(await screen.findByText("中国历史与文化")).toBeInTheDocument();
  expect(screen.getByText(/验证会话中/)).toBeInTheDocument();
});

it("keeps a successful but restricted login separate from captcha failure", async () => {
  auth = "required";
  const original = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((name, args) => {
    if (
      name === "enrollment_command" &&
      ((args as Record<string, unknown>).action as { type: string }).type === "captchaPrepare"
    )
      return Promise.resolve({
        status: "authenticated",
        snapshot: {
          ...snapshot,
          loggedIn: true,
          authentication: "authenticated",
          admission: "restricted",
          admissionMessage: "抱歉,未评教,学籍信息不对,无法选课!",
        },
      });
    return original(name, args);
  });
  const router = mount("/enrollment/login");
  fireEvent.click(await screen.findByRole("button", { name: "登录并进入选课" }));
  await screen.findByText("登录成功，当前暂不可选课");
  expect(screen.getByText("抱歉,未评教,学籍信息不对,无法选课!")).toBeInTheDocument();
  expect(screen.queryByLabelText("点选验证码")).toBeNull();
  expect(router.state.location.pathname).toBe("/enrollment/login");
  expect(
    vi
      .mocked(invoke)
      .mock.calls.filter(
        ([name, args]) =>
          name === "enrollment_command" &&
          ((args as Record<string, unknown>).action as { type: string }).type === "captchaPrepare",
      ),
  ).toHaveLength(1);
});
it("reads and navigates cache without any enrollment network commands", async () => {
  auth = "required";
  const router = mount("/enrollment?view=cache");
  await screen.findByText("中国历史与文化");
  expect(screen.getByRole("button", { name: "检索缓存" })).toBeInTheDocument();
  expect(
    screen
      .getAllByRole("button", { name: "选课 / 加入待抢" })
      .every((b) => (b as HTMLButtonElement).disabled),
  ).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "抢课任务" }));
  await screen.findByRole("heading", { name: "抢课任务" });
  expect(router.state.location.searchStr).toContain("view=cache");
  expect(screen.getByRole("button", { name: "开始 / 继续" })).toBeDisabled();
  const calls = vi
    .mocked(invoke)
    .mock.calls.filter(([name]) => name === "enrollment_command")
    .map(([, args]) => ((args as Record<string, unknown>).action as { type: string }).type);
  expect(calls.every((type) => ["cacheSnapshot", "cache"].includes(type))).toBe(true);
});
it("activates the entire radio card once when its blank area is clicked", async () => {
  auth = "required";
  mount("/enrollment/login");
  const radio = await screen.findByRole("radio", { name: "WebVPN访问" });
  await screen.findByText("需2FA验证");
  fireEvent.click(radio.closest("label")!);
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("open_academic_portal", { kind: "webvpn", target: null }),
  );
  expect(
    vi
      .mocked(invoke)
      .mock.calls.filter(
        ([name, args]) =>
          name === "enrollment_command" &&
          ((args as Record<string, unknown>).action as { type: string }).type === "mode",
      ),
  ).toHaveLength(1);
  expect(
    vi.mocked(invoke).mock.calls.filter(([name]) => name === "open_academic_portal"),
  ).toHaveLength(1);
});

it("reopens 2FA once from the selected card latency without resetting its mode", async () => {
  auth = "required";
  mount("/enrollment/login");
  const radio = await screen.findByRole("radio", { name: "WebVPN访问" });
  await screen.findByText("需2FA验证");
  fireEvent.click(radio.closest("label")!);
  await waitFor(() => expect(radio).toBeChecked());
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("open_academic_portal", { kind: "webvpn", target: null }),
  );
  vi.mocked(invoke).mockClear();
  fireEvent.click(screen.getByText("需2FA验证"));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("open_academic_portal", { kind: "webvpn", target: null }),
  );
  expect(
    vi.mocked(invoke).mock.calls.filter(([name]) => name === "open_academic_portal"),
  ).toHaveLength(1);
  expect(
    vi
      .mocked(invoke)
      .mock.calls.filter(
        ([name, args]) =>
          name === "enrollment_command" &&
          ((args as Record<string, unknown>).action as { type: string }).type === "mode",
      ),
  ).toHaveLength(0);
});

it("never renders a standalone zero through repeated login requests", async () => {
  auth = "required";
  const original = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((name, args) => {
    if (
      name === "enrollment_command" &&
      ((args as Record<string, unknown>).action as { type: string }).type === "captchaPrepare"
    )
      return Promise.reject("学校暂不可用");
    return original(name, args);
  });
  mount("/enrollment/login");
  const button = await screen.findByRole("button", { name: "登录并进入选课" });
  const hasZero = () => {
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    while (walker.nextNode())
      if (
        walker.currentNode.textContent?.trim() === "0" &&
        !walker.currentNode.parentElement?.closest('[aria-label="验证码重试次数"]')
      )
        return true;
    return false;
  };
  expect(hasZero()).toBe(false);
  fireEvent.click(button);
  fireEvent.click(button);
  await screen.findByText("学校暂不可用");
  expect(hasZero()).toBe(false);
});
