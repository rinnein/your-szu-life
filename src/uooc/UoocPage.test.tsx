/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createMemoryHistory, createRouter, RouterProvider } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { routeTree } from "../routeTree.gen";
import { initialLogicalEntries } from "../logical-navigation";
import {
  newerSnapshot,
  type UoocSnapshot,
  type UoocTaskStatus,
  type CourseProgress,
} from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
// jsdom does not implement the Web Animations API used by HeroUI tab indicators.
Element.prototype.getAnimations = () => [];
let snapshot: UoocSnapshot;
let onState: EventCallback<UoocSnapshot> | undefined;
const stop = vi.fn();
function mount(path = "/uooc") {
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
  onState = undefined;
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
  vi.spyOn(window, "scrollTo").mockImplementation(() => undefined);
  snapshot = {
    revision: 1,
    session: "ready",
    user: { id: "7", name: "测试用户" },
    message: "",
    logs: [],
    tasks: [],
    settings: { concurrency: 3, reportIntervalSec: 20, speedMultiplier: 2, dryRun: false },
    courses: [
      {
        id: "1588976494",
        title: "人工智能",
        organization: "深圳大学",
        semester: "秋季",
        weeks: "5/7",
      },
      {
        id: "669673711",
        title: "艺术的星空",
        organization: "南昌大学",
        semester: "秋季",
        weeks: "5/17",
      },
    ],
  };
  vi.mocked(listen).mockImplementation(async (name, callback) => {
    if (name === "uooc-state-changed") onState = callback as EventCallback<UoocSnapshot>;
    return stop;
  });
  vi.mocked(invoke).mockImplementation(async (name) => {
    if (
      name === "get_uooc_snapshot" ||
      name === "start_uooc_courses" ||
      name === "save_uooc_settings" ||
      name === "control_uooc_tasks"
    )
      return structuredClone(snapshot);
    if (name === "get_auth_state")
      return { Authenticated: { studentId: "1234567890", name: "测试同学" } };
    if (name === "get_account_profile")
      return { studentId: "1234567890", campusCardNumber: "123456", hasPassword: true };
    if (name === "get_user_data") return { studentId: "1234567890", name: "测试同学" };
    return undefined;
  });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  delete window.SzuApplicationBack;
});

it("opens from workbench, selects cycle IDs, starts multiple courses, and replaces tabs", async () => {
  const router = mount("/");
  fireEvent.click(await screen.findByRole("button", { name: /^UOOC刷课/ }));
  const first = await screen.findByRole("checkbox", { name: "人工智能" });
  fireEvent.click(first);
  fireEvent.click(screen.getByRole("checkbox", { name: "艺术的星空" }));
  fireEvent.click(screen.getByRole("button", { name: "开始所选 (2)" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("start_uooc_courses", {
      courseIds: ["1588976494", "669673711"],
    }),
  );
  await waitFor(() => expect(router.state.location.search).toEqual({ tab: "tasks" }));
  fireEvent.click(screen.getByRole("tab", { name: "运行设置" }));
  await waitFor(() => expect(router.state.location.search).toEqual({ tab: "settings" }));
  fireEvent.click(screen.getByRole("button", { name: "返回工作台" }));
  await waitFor(() => expect(router.state.location.pathname).toBe("/"));
  expect(router.history.canGoBack()).toBe(false);
  expect(invoke).not.toHaveBeenCalledWith("control_uooc_tasks", expect.anything());
});

it("subscribes to native state, rejects stale events, and controls tasks without credentials", async () => {
  snapshot.tasks = [
    {
      course: snapshot.courses[0],
      accountId: "7",
      desiredRunning: true,
      generation: 1,
      status: "running",
      message: "",
      videos: [
        {
          resourceId: "100",
          title: "视频一",
          status: "running",
          position: 30,
          duration: 120,
          message: "",
        },
      ],
    },
  ];
  mount("/uooc?tab=tasks");
  expect(await screen.findByText("视频一")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "暂停全部" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("control_uooc_tasks", { courseIds: [], action: "pause" }),
  );
  await act(async () =>
    onState?.({
      event: "uooc-state-changed",
      id: 1,
      payload: { ...snapshot, revision: 10, message: "最新状态" },
    }),
  );
  await act(async () =>
    onState?.({
      event: "uooc-state-changed",
      id: 1,
      payload: { ...snapshot, revision: 9, message: "过期状态" },
    }),
  );
  expect(screen.getByText("最新状态")).toBeInTheDocument();
  expect(screen.queryByText("过期状态")).not.toBeInTheDocument();
  expect(newerSnapshot({ ...snapshot, revision: 10 }, snapshot).revision).toBe(10);
});

it("uses native Android back and exposes official login without a Cookie input", async () => {
  snapshot.session = "loginRequired";
  snapshot.user = null;
  snapshot.courses = [];
  const bridge = { postMessage: vi.fn(), onmessage: null } as NonNullable<
    Window["SzuApplicationBack"]
  >;
  window.SzuApplicationBack = bridge;
  const router = mount();
  const login = await screen.findByRole("button", { name: "登录 UOOC" });
  await waitFor(() => expect(login).toBeEnabled());
  fireEvent.click(login);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("open_uooc_login", undefined));
  expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "返回工作台" }));
  expect(bridge.postMessage).toHaveBeenCalledWith("back-request");
  await act(async () => bridge.onmessage?.({ data: "navigate-back" }));
  await waitFor(() => expect(router.state.location.pathname).toBe("/"));
});

it("saves preview settings and shows command errors", async () => {
  mount("/uooc?tab=settings");
  const preview = await screen.findByRole("checkbox", { name: /仅预览/ });
  await waitFor(() => expect(preview).toBeEnabled());
  fireEvent.click(preview);
  fireEvent.click(screen.getByRole("button", { name: "保存设置" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("save_uooc_settings", {
      settings: { ...snapshot.settings, dryRun: true },
    }),
  );
  vi.mocked(invoke).mockRejectedValueOnce("网络不可用");
  await waitFor(() => expect(screen.getByRole("button", { name: "刷新课程" })).toBeEnabled());
  fireEvent.click(screen.getByRole("button", { name: "刷新课程" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("网络不可用");
});

it("does not label a real run as a preview when the settings edit has not been saved", async () => {
  const router = mount("/uooc?tab=settings");
  const preview = await screen.findByRole("checkbox", { name: /仅预览/ });
  await waitFor(() => expect(preview).toBeEnabled());
  fireEvent.click(preview);
  fireEvent.click(screen.getByRole("tab", { name: "开课中" }));
  await waitFor(() => expect(router.state.location.search).toEqual({ tab: "courses" }));
  expect(screen.getByRole("button", { name: "开始所选 (0)" })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /预览所选/ })).not.toBeInTheDocument();
});

function existingTask(status: UoocTaskStatus = "paused") {
  return {
    course: snapshot.courses[0],
    accountId: "7",
    desiredRunning: false,
    generation: 8,
    status,
    message: "",
    videos: [
      {
        resourceId: "101",
        title: "视频一",
        status: "completed" as const,
        position: 79,
        duration: 80,
        message: "",
      },
    ],
  };
}
function progress(attendance: CourseProgress["attendance"] = "needed"): CourseProgress {
  return {
    totalPercent: 35,
    video: { percent: 80, count: 8, total: 10 },
    test: { percent: 20, count: 2, total: 10 },
    homework: { percent: null, count: null, total: null },
    signin: { percent: 4, count: 1, total: 25 },
    signinWeight: 5,
    signinTime: null,
    attendance,
  };
}

it.each<UoocTaskStatus>([
  "queued",
  "running",
  "paused",
  "stopped",
  "failed",
  "completed",
  "dryRun",
  "blocked",
])("disables an already-added %s course and excludes it from select-all", async (status) => {
  snapshot.tasks = [existingTask(status)];
  mount();
  const checkbox = await screen.findByRole("checkbox", { name: "人工智能" });
  await waitFor(() => expect(checkbox).toBeChecked());
  expect(checkbox).toBeDisabled();
  expect(screen.getByText("已添加")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "全选" }));
  fireEvent.click(screen.getByRole("button", { name: "开始所选 (1)" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("start_uooc_courses", { courseIds: ["669673711"] }),
  );
});

it("marks server-completed videos green without changing their reported position", async () => {
  snapshot.tasks = [existingTask("completed")];
  snapshot.progress = {
    "1588976494": { data: progress(), updatedAtMs: 123, refreshing: false, error: null },
  };
  mount("/uooc?tab=tasks");
  expect(await screen.findByText("已刷完")).toHaveClass("chip__label");
  expect(screen.getByText("已刷完").closest('[data-slot="chip"]')).toHaveClass("chip--success");
  expect(screen.getByRole("progressbar", { name: "视频一进度" })).toHaveAttribute(
    "aria-valuenow",
    "100",
  );
  expect(screen.getByRole("progressbar", { name: "课程总进度" })).toHaveAttribute(
    "aria-valuenow",
    "35",
  );
  expect(screen.getByText("视频已刷完")).toBeInTheDocument();
  expect(screen.getByText(/1:19 \/ 1:20/)).toBeInTheDocument();
});

it("does not turn dry-run completion into a green finished video", async () => {
  const task = existingTask("dryRun");
  snapshot.tasks = [{ ...task, videos: [{ ...task.videos[0], status: "dryRun" }] }];
  mount("/uooc?tab=tasks");
  await screen.findByText("视频一");
  expect(screen.queryByText("已刷完")).not.toBeInTheDocument();
  expect(screen.queryByText("视频已刷完")).not.toBeInTheDocument();
});

it("shows server progress and allows check-in for a course whose add checkbox is disabled", async () => {
  snapshot.tasks = [existingTask()];
  snapshot.progress = {
    "1588976494": { data: progress(), updatedAtMs: 123, refreshing: false, error: null },
    "669673711": { data: progress("today"), updatedAtMs: 123, refreshing: false, error: null },
  };
  mount();
  const signup = await screen.findByRole("button", { name: "签到" });
  expect(screen.getByRole("checkbox", { name: "人工智能" })).toBeDisabled();
  expect(screen.getAllByText("35%")).toHaveLength(2);
  expect(screen.getByText("今日已签到")).toBeInTheDocument();
  fireEvent.click(signup);
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("start_uooc_checkin", { courseIds: ["1588976494"] }),
  );
  expect(invoke).toHaveBeenCalledWith("refresh_uooc_progress", { courseIds: [] });
});

it("cancels check-in without stopping video tasks and retains progress on refresh errors", async () => {
  snapshot.progress = {
    "1588976494": { data: progress(), updatedAtMs: 123, refreshing: false, error: "暂时离线" },
  };
  snapshot.checkins = {
    "1588976494": {
      id: "checkin-1",
      status: "verifying",
      visible: false,
      message: "等待服务端确认",
      updatedAtMs: 123,
    },
  };
  mount();
  fireEvent.click(await screen.findByRole("button", { name: "取消签到" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("cancel_uooc_checkin", { courseIds: ["1588976494"] }),
  );
  expect(invoke).not.toHaveBeenCalledWith("control_uooc_tasks", expect.anything());
  expect(screen.getByText("35%")).toBeInTheDocument();
  expect(screen.getByText(/进度刷新失败：暂时离线/)).toBeInTheDocument();
});

it("refreshes the selected course when opening its details without adding a navigation level", async () => {
  const router = mount();
  const trigger = await screen.findByRole("button", { name: "人工智能的课程详情" });
  await screen.findByRole("checkbox", { name: "人工智能" });
  fireEvent.click(trigger);
  await screen.findByRole("grid", { name: "人工智能的考核权重与任务" });
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("refresh_uooc_progress", { courseIds: ["1588976494"] }),
  );
  expect(router.state.location.pathname).toBe("/uooc");
  fireEvent.click(screen.getByRole("button", { name: "返回工作台" }));
  await waitFor(() => expect(router.state.location.pathname).toBe("/"));
});
