/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createMemoryHistory, createRouter, RouterProvider } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { routeTree } from "../routeTree.gen";
import { initialLogicalEntries } from "../logical-navigation";
import {
  newerSnapshot,
  buildCourseTaskItems,
  isCourseTaskCompleted,
  type UoocSnapshot,
  type UoocTaskStatus,
  type CourseProgress,
  type QuizState,
  type UoocVideo,
} from "./types";

vi.setConfig({ testTimeout: 15000 });
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
    settings: {
      concurrency: 3,
      reportIntervalSec: 20,
      speedMultiplier: 2,
      dryRun: false,
      discussionEnabled: true,
      discussionContent: "要好好学习",
      discussionIntervalMin: 30,
    },
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
    if (name === "get_uooc_ai_config")
      return {
        main: { baseUrl: "https://example.org/v1", model: "main", hasKey: true },
        jev: { baseUrl: "https://api.typesafe.ai/v1", model: "jev", hasKey: false },
        jevEnabled: false,
        confidence: 0.8,
        defaultMode: "supervised",
      };
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

it("opens global AI connection settings from UOOC runtime settings", async () => {
  const router = mount("/uooc?tab=settings");
  await screen.findByRole("button", { name: "前往 AI 连接设置" });
  fireEvent.click(screen.getByRole("button", { name: "前往 AI 连接设置" }));
  await waitFor(() => expect(router.state.location.pathname).toBe("/settings"));
});

it("does not display qa server url field on settings tab", async () => {
  mount("/uooc?tab=settings");
  await screen.findByRole("button", { name: "保存设置" });
  expect(screen.queryByLabelText(/题库共享后端地址/)).not.toBeInTheDocument();
});

it("displays and saves qa server url field on qa tab", async () => {
  vi.mocked(invoke).mockImplementation(async (name) => {
    if (name === "get_debug_mode") return { enabled: true, logCount: 0 };
    if (
      name === "get_uooc_snapshot" ||
      name === "start_uooc_courses" ||
      name === "save_uooc_settings" ||
      name === "control_uooc_tasks"
    )
      return structuredClone(snapshot);
    if (name === "get_uooc_embedding_status")
      return {
        status: "ready",
        cachedCount: 10,
        totalDimension: 1536,
        updatedAtMs: 1,
        message: "",
      };
    if (name === "get_uooc_qa_account_status")
      return {
        isAnonymous: true,
        permissionLevel: "anonymous",
        canOverwrite: false,
      };
    if (name === "get_uooc_local_bank_questions") return [];
    if (name === "get_uooc_ai_config")
      return {
        main: { baseUrl: "https://example.org/v1", model: "main", hasKey: true },
        jev: { baseUrl: "https://api.typesafe.ai/v1", model: "jev", hasKey: false },
        jevEnabled: false,
        confidence: 0.8,
        defaultMode: "supervised",
      };
    return undefined;
  });

  mount("/uooc?tab=qa");
  const qaInput = await screen.findByLabelText(/题库共享后端地址/);
  expect(qaInput).toBeInTheDocument();
  expect(qaInput).toHaveValue("http://localhost:4321");

  fireEvent.change(qaInput, { target: { value: "http://custom-qa.example:5000" } });
  fireEvent.click(screen.getByRole("button", { name: "保存题库地址" }));

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("save_uooc_settings", {
      settings: {
        ...snapshot.settings,
        qaServerUrl: "http://custom-qa.example:5000",
      },
    }),
  );
});

it("starts local bank build immediately when entering uooc page with ready session", async () => {
  mount("/uooc");
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("build_uooc_local_bank"));
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
  expect(await screen.findByText("视频已刷完")).toBeInTheDocument();
  const accordionTrigger = screen.getByRole("button", { name: "已刷完任务 (1 项)" });
  fireEvent.click(accordionTrigger);
  const completedChips = await screen.findAllByText("已刷完");
  expect(completedChips.length).toBeGreaterThanOrEqual(1);
  expect(completedChips[0].closest('[data-slot="chip"]')).toHaveClass("chip--success");
  expect(screen.getByRole("progressbar", { name: "视频一进度" })).toHaveAttribute(
    "aria-valuenow",
    "100",
  );
  expect(screen.getByRole("progressbar", { name: "课程总进度" })).toHaveAttribute(
    "aria-valuenow",
    "35",
  );
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

it.each(["submitting", "verifying"] as const)(
  "cancels %s check-in without stopping video tasks and retains progress on refresh errors",
  async (status) => {
    snapshot.progress = {
      "1588976494": { data: progress(), updatedAtMs: 123, refreshing: false, error: "暂时离线" },
    };
    snapshot.checkins = {
      "1588976494": {
        id: "checkin-1",
        status,
        source: "automatic",
        message: "等待服务端确认",
        updatedAtMs: 123,
      },
    };
    mount();
    expect(await screen.findByText("自动签到：等待服务端确认")).toBeInTheDocument();
    fireEvent.click(await screen.findByRole("button", { name: "取消签到" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("cancel_uooc_checkin", { courseIds: ["1588976494"] }),
    );
    expect(invoke).not.toHaveBeenCalledWith("control_uooc_tasks", expect.anything());
    expect(screen.getByText("35%")).toBeInTheDocument();
    expect(screen.getByText(/进度刷新失败：暂时离线/)).toBeInTheDocument();
  },
);

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

it("enables quiz mode picker only when test weight is positive, defaulting to 手动答题", async () => {
  snapshot.progress = {
    "1588976494": {
      data: {
        ...progress(),
        test: { percent: 10, count: 1, total: 10, weight: 20, score: 2 },
      },
      updatedAtMs: 123,
      refreshing: false,
      error: null,
    },
    "669673711": {
      data: {
        ...progress(),
        test: { percent: 0, count: 0, total: 10, weight: 0, score: 0 },
      },
      updatedAtMs: 123,
      refreshing: false,
      error: null,
    },
  };
  mount("/uooc?tab=courses");
  await screen.findByRole("checkbox", { name: "人工智能" });
  const pickers = screen
    .getAllByRole("button")
    .filter((b) => b.getAttribute("aria-label") === "答题模式");
  expect(pickers).toHaveLength(2);
  // Course 1 has test weight 20 -> enabled
  expect(pickers[0]).toBeEnabled();
  expect(pickers[0]).toHaveTextContent("手动答题");
  // Course 2 has test weight 0 -> disabled
  expect(pickers[1]).toBeDisabled();
  expect(screen.getByText("测验无权重占比，无需配置答题模式")).toBeInTheDocument();
});

it("automatically collapses completed tasks in taskItems into a single item, keeps active tasks expanded, and toggles collapse", async () => {
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
          resourceId: "v-completed",
          title: "已完成视频",
          status: "completed",
          position: 60,
          duration: 60,
          message: "",
        },
        {
          resourceId: "v-running",
          title: "进行中视频",
          status: "running",
          position: 10,
          duration: 100,
          message: "",
        },
      ],
    },
  ];
  mount("/uooc?tab=tasks");
  // Active task is shown directly
  expect(await screen.findByText("进行中视频")).toBeInTheDocument();

  // Completed item is compressed and folded into an Accordion
  const accordionTrigger = screen.getByRole("button", { name: "已刷完任务 (1 项)" });
  expect(accordionTrigger).toHaveAttribute("aria-expanded", "false");

  // Expanding the accordion
  fireEvent.click(accordionTrigger);
  expect(accordionTrigger).toHaveAttribute("aria-expanded", "true");
  expect(await screen.findByText("已完成视频")).toBeInTheDocument();

  // Folding compresses it back
  fireEvent.click(accordionTrigger);
  expect(accordionTrigger).toHaveAttribute("aria-expanded", "false");
});

it("orders video tasks and quiz tasks sequentially and opens quiz modal on click", async () => {
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
          resourceId: "v1",
          title: "视频一",
          status: "completed",
          position: 60,
          duration: 60,
          message: "",
        },
        {
          resourceId: "v2",
          title: "视频二",
          status: "queued",
          position: 0,
          duration: 120,
          message: "",
        },
      ],
    },
  ];
  snapshot.quizzes = {
    "7:1588976494": {
      accountId: "7",
      cid: "1588976494",
      taskId: "task-quiz-1",
      resourceId: "res-quiz-1",
      title: "第一章测验",
      mode: "supervised",
      stage: "awaitingChoice",
      revision: 1,
      questions: [
        {
          id: "q1",
          text: "1+1等于几？",
          kind: "single",
          options: [
            { id: "opt1", text: "2" },
            { id: "opt2", text: "3" },
          ],
          blanks: 0,
          images: [],
        },
      ],
      answers: {},
      reviews: [],
      message: "请选择自主答题或 AI 答题",
      gated: true,
      videoComplete: false,
      passed: [],
    },
  };
  snapshot.progress = {
    "1588976494": {
      data: {
        ...progress(),
        test: { percent: 0, count: 0, total: 10, weight: 20, score: 0 },
      },
      updatedAtMs: 123,
      refreshing: false,
      error: null,
    },
  };
  mount("/uooc?tab=tasks");
  // Quiz task appears in the list between completed video and queued video
  const quizRow = await screen.findByRole("button", {
    name: "测验：第一章测验，点击打开测验",
  });
  expect(quizRow).toBeInTheDocument();
  expect(screen.getByText("视频二")).toBeInTheDocument();
  const accordionTrigger = screen.getByRole("button", { name: "已刷完任务 (1 项)" });
  expect(accordionTrigger).toBeInTheDocument();
  fireEvent.click(accordionTrigger);
  expect(await screen.findByText("视频一")).toBeInTheDocument();

  // Click quiz row to open modal
  fireEvent.click(quizRow);
  const modal = await screen.findByRole("dialog", { name: "第一章测验" });
  expect(modal).toBeInTheDocument();
  expect(within(modal).getByRole("button", { name: "自主答题" })).toBeInTheDocument();
  expect(within(modal).getByRole("button", { name: "AI 答题" })).toBeInTheDocument();
});

it("buildCourseTaskItems orders tasks correctly under various progression states", () => {
  const v1: UoocVideo = {
    resourceId: "1",
    title: "V1",
    status: "completed",
    position: 10,
    duration: 10,
    message: "",
  };
  const v2: UoocVideo = {
    resourceId: "2",
    title: "V2",
    status: "queued",
    position: 0,
    duration: 10,
    message: "",
  };
  const v3: UoocVideo = {
    resourceId: "3",
    title: "V3",
    status: "queued",
    position: 0,
    duration: 10,
    message: "",
  };
  const baseQuiz: QuizState = {
    accountId: "1",
    cid: "42",
    taskId: "t1",
    resourceId: "q1",
    title: "Q1",
    mode: "supervised",
    stage: "awaitingChoice",
    revision: 1,
    questions: [],
    answers: {},
    reviews: [],
    message: "",
    gated: false,
    videoComplete: false,
    passed: [],
  };

  // 1. No quiz
  expect(buildCourseTaskItems([v1, v2], undefined)).toEqual([
    { id: "video-1", kind: "video", video: v1 },
    { id: "video-2", kind: "video", video: v2 },
  ]);

  // 2. Disabled quiz is omitted
  expect(buildCourseTaskItems([v1, v2], { ...baseQuiz, stage: "disabled" })).toHaveLength(2);

  // 3. Idle quiz without taskId or title is omitted
  expect(
    buildCourseTaskItems([v1, v2], { ...baseQuiz, stage: "idle", taskId: "", title: "" }),
  ).toHaveLength(2);

  // 4. Gated quiz placed after completed videos and before queued videos
  const gated = buildCourseTaskItems([v1, v2, v3], { ...baseQuiz, gated: true });
  expect(gated.map((item) => item.id)).toEqual(["video-1", "quiz-42-q1", "video-2", "video-3"]);

  // 5. VideoComplete quiz placed at end
  const end = buildCourseTaskItems([v1, v2], { ...baseQuiz, videoComplete: true });
  expect(end.map((item) => item.id)).toEqual(["video-1", "video-2", "quiz-42-q1"]);

  // 6. All videos queued -> quiz placed at start
  const allQueued = buildCourseTaskItems([v2, v3], { ...baseQuiz, gated: true });
  expect(allQueued.map((item) => item.id)).toEqual(["quiz-42-q1", "video-2", "video-3"]);

  // 7. No videos -> quiz alone
  const noVideos = buildCourseTaskItems([], baseQuiz);
  expect(noVideos.map((item) => item.id)).toEqual(["quiz-42-q1"]);
});

it("isCourseTaskCompleted recognizes completed status, 100% video progress, or all completed videos", () => {
  const task = existingTask("running");
  expect(isCourseTaskCompleted(task, undefined)).toBe(true); // task.videos has 1 completed video
  const uncompletedTask = {
    ...task,
    videos: [{ ...task.videos[0], status: "queued" as const }],
  };
  expect(isCourseTaskCompleted(uncompletedTask, undefined)).toBe(false);
  expect(isCourseTaskCompleted({ ...uncompletedTask, status: "completed" }, undefined)).toBe(true);
  expect(
    isCourseTaskCompleted(uncompletedTask, {
      data: { ...progress(), video: { percent: 100, count: 10, total: 10 } },
      updatedAtMs: 123,
      refreshing: false,
      error: null,
    }),
  ).toBe(true);
});
