/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import {
  act,
  cleanup,
  configure,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { createMemoryHistory, createRouter, RouterProvider } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { routeTree } from "../routeTree.gen";
import { typeInto } from "../ui/testing";
import { localDateKey, type RollcallItem } from "./types";

// 认证/初始化链路在 jsdom 里要走多个 invoke，默认 1s 超时不够。
configure({ asyncUtilTimeout: 5000 });

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class MockChannel {
    onmessage = null;
  },
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

type Listener = (event: { payload: unknown }) => void;
const listeners = new Map<string, Listener>();

vi.mock("../os-info", () => ({
  isAndroidPlatform: vi.fn(async () => false),
  getPlatform: vi.fn(async () => "linux"),
  getOsType: vi.fn(async () => "Linux"),
  getOsVersion: vi.fn(async () => "test"),
  getArch: vi.fn(async () => "x86_64"),
}));

const buildings = [
  {
    id: 79,
    name: "致理楼",
    name_en: "Zhili Building",
    campus: "沧海校区",
    latitude_gcj02: 22.528,
    longitude_gcj02: 113.94,
    radius_m: 130,
    status: "listed",
  },
];

// 字段名与后端 serde 输出一致（嵌套条目 snake_case，类型键是 `type`）。
const rollcalls: RollcallItem[] = [
  {
    rollcall_id: 123,
    is_number: true,
    is_radar: false,
    course_id: 9,
    course_title: "数据库系统",
    student_rollcall_id: 1,
    type: "number",
    status: "on_call",
  },
  {
    rollcall_id: 456,
    is_number: false,
    is_radar: true,
    course_id: 10,
    course_title: "概率论与数理统计",
    student_rollcall_id: 2,
    type: "radar",
    status: "on_call",
  },
  {
    rollcall_id: 789,
    is_number: false,
    is_radar: false,
    course_id: 11,
    course_title: "大学英语",
    student_rollcall_id: 3,
    type: "qr",
    status: "on_call",
  },
];

function todaySession() {
  return {
    date: localDateKey(),
    course_name: "概率论与数理统计",
    building_name: "致理楼",
    building_name_en: "Zhili Building",
    classroom: "致理楼L1-203",
    start_ms: Date.now() - 600_000,
    end_ms: Date.now() + 600_000,
    semester: "2026-2027-1",
  };
}

let submitted: Array<{ command: string; args: Record<string, unknown> }>;

function defaultInvoke(command: string, args?: unknown): Promise<unknown> {
  switch (command) {
    case "get_auth_state":
      return Promise.resolve({
        Authenticated: { studentId: "20230001", name: "测试" },
      });
    case "get_onboarding_state":
      return Promise.resolve({ steps: [], needsSetup: false, currentStep: null });
    case "signin_get_buildings":
      return Promise.resolve(buildings);
    case "signin_get_qw_status":
      return Promise.resolve({ state: "ready", message: "" });
    case "signin_get_lms_rollcalls":
      return Promise.resolve(rollcalls);
    case "signin_resolve_radar_point":
      return Promise.resolve({
        latitude: 22.528,
        longitude: 113.94,
        label: "致理楼",
        source: "fallback",
        fenceRadiusM: 130,
      });
    case "signin_get_timetable":
      return Promise.resolve({
        fetched_at_ms: 1,
        semester: "2026-2027-1",
        sessions: [todaySession()],
      });
    case "signin_get_last_photo":
      return Promise.resolve({ hasPhoto: false });
    case "signin_submit_lms_number":
    case "signin_submit_lms_radar":
    case "signin_submit_lms_qr":
    case "signin_submit_szu":
      submitted.push({ command, args: (args ?? {}) as Record<string, unknown> });
      return Promise.resolve(
        command === "signin_submit_lms_radar"
          ? { latitude: 22.528, longitude: 113.94, label: "致理楼", source: "fallback" }
          : null,
      );
    default:
      return Promise.resolve(undefined);
  }
}

function setupInvoke() {
  submitted = [];
  vi.mocked(invoke).mockImplementation(defaultInvoke as unknown as typeof invoke);
}

async function mount(initial: string) {
  const router = createRouter({
    routeTree,
    history: createMemoryHistory({ initialEntries: [initial] }),
  });
  render(<RouterProvider router={router} />);
  return router;
}

beforeEach(() => {
  vi.clearAllMocks();
  listeners.clear();
  vi.mocked(listen).mockImplementation((async (event: string, handler: Listener) => {
    listeners.set(event, handler);
    return () => listeners.delete(event);
  }) as unknown as typeof listen);
  setupInvoke();
});

afterEach(cleanup);

it("lists pending rollcalls and submits a number code from the modal", async () => {
  await mount("/study/signin");
  expect(await screen.findByText("数据库系统")).toBeInTheDocument();

  // 点击签到行打开模态框（数字签到）。
  await act(async () => {
    fireEvent.click(screen.getByText("数据库系统"));
  });
  expect(await screen.findByLabelText("输入签到数字码")).toBeInTheDocument();

  typeInto(screen.getByLabelText("输入签到数字码"), "0837");
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "提交" }));
  });

  await waitFor(() => {
    expect(submitted.some((entry) => entry.command === "signin_submit_lms_number")).toBe(true);
  });
  const entry = submitted.find((item) => item.command === "signin_submit_lms_number")!;
  expect(entry.args.rollcallId).toBe(123);
  expect(entry.args.code).toBe("0837");
}, 15000);

it("ignores duplicate submit triggers while a submission is pending", async () => {
  await mount("/study/signin");
  fireEvent.click(await screen.findByText("数据库系统"));
  const field = await screen.findByLabelText("输入签到数字码");
  typeInto(field, "0837");
  const submitButton = screen.getByRole("button", { name: "提交" });
  await act(async () => {
    fireEvent.click(submitButton);
    fireEvent.click(submitButton);
  });
  await waitFor(() => expect(submitted).toHaveLength(1));
}, 15000);

it("radar modal resolves the default point and submits without coordinate args", async () => {
  await mount("/study/signin");
  fireEvent.click(await screen.findByText("概率论与数理统计"));
  // 地点选择出现：默认点标签与提交按钮。
  expect(await screen.findByTestId("point-label")).toHaveTextContent("致理楼");
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "提交" }));
  });
  await waitFor(() => {
    expect(submitted.some((entry) => entry.command === "signin_submit_lms_radar")).toBe(true);
  });
  // 未改动下拉 → 不携带 buildingId/custom，走后端默认解析链。
  const entry = submitted.find((item) => item.command === "signin_submit_lms_radar")!;
  expect(entry.args.rollcallId).toBe(456);
  expect(entry.args.buildingId).toBeUndefined();
  expect(entry.args.customLatitude).toBeUndefined();
}, 15000);

it("renders the SZU tab with today's sessions", async () => {
  await mount("/study/signin?tab=szu");
  expect(await screen.findByText("概率论与数理统计")).toBeInTheDocument();
  expect(screen.getByRole("list", { name: "今天的课次" })).toBeInTheDocument();
  expect(screen.queryByText("数据库系统")).not.toBeInTheDocument();
}, 15000);

it("disables SZU submit until a photo is available", async () => {
  await mount("/study/signin?tab=szu");
  fireEvent.click(await screen.findByText("概率论与数理统计"));
  expect(await screen.findByTestId("point-label")).toHaveTextContent("致理楼");
  // 无本次照片且无上次照片 → 提交不可用，而不是提交后撞服务端错误。
  expect(screen.getByRole("button", { name: "提交" })).toBeDisabled();
  expect(await screen.findByText("SZU 签到需要照片，请先拍摄或选择一张。")).toBeInTheDocument();
}, 15000);

it("submits SZU signin with the remembered photo", async () => {
  vi.mocked(invoke).mockImplementation(async (command: string, args?: unknown) => {
    if (command === "signin_get_last_photo") {
      return { hasPhoto: true, thumbnail: "data:image/jpeg;base64,QUJD" };
    }
    return defaultInvoke(command, args);
  });
  await mount("/study/signin?tab=szu");
  fireEvent.click(await screen.findByText("概率论与数理统计"));
  expect(await screen.findByTestId("point-label")).toHaveTextContent("致理楼");
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "提交" }));
  });
  await waitFor(() => {
    expect(submitted.some((entry) => entry.command === "signin_submit_szu")).toBe(true);
  });
  const entry = submitted.find((item) => item.command === "signin_submit_szu")!;
  expect(entry.args.courseName).toBe("概率论与数理统计");
  expect(entry.args.buildingId).toBeUndefined();
  // 没有重新拍照时不携带 photoB64（用课程记忆的上次照片）。
  expect(entry.args.photoB64).toBeUndefined();
}, 15000);

it("submits SZU signin with a freshly picked photo", async () => {
  await mount("/study/signin?tab=szu");
  fireEvent.click(await screen.findByText("概率论与数理统计"));
  const input = await screen.findByTestId("szu-photo-file-input");
  fireEvent.change(input, {
    target: { files: [new File(["abc"], "photo.jpg", { type: "image/jpeg" })] },
  });
  expect(await screen.findByText(/将使用本次选择的照片/)).toBeInTheDocument();
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "提交" }));
  });
  await waitFor(() => {
    expect(submitted.some((entry) => entry.command === "signin_submit_szu")).toBe(true);
  });
  const entry = submitted.find((item) => item.command === "signin_submit_szu")!;
  // data URI 前缀在提交时切掉，只传 base64。
  expect(entry.args.photoB64).toBe("YWJj");
}, 15000);

it("shows an error state with retry when the rollcall list fails to load", async () => {
  vi.mocked(invoke).mockImplementation(async (command: string, args?: unknown) => {
    if (command === "signin_get_lms_rollcalls") throw "考勤探测失败";
    return defaultInvoke(command, args);
  });
  await mount("/study/signin");
  // 读取失败呈现错误态，而不是渲染成「没有待签到」的空态。
  expect(await screen.findByText("无法读取待签到列表")).toBeInTheDocument();
  expect(await screen.findByText("考勤探测失败")).toBeInTheDocument();
  expect(screen.queryByText("当前没有待签到的点名")).not.toBeInTheDocument();

  // 恢复后重试能拿到列表。
  setupInvoke();
  fireEvent.click(screen.getByRole("button", { name: "重试" }));
  expect(await screen.findByText("数据库系统")).toBeInTheDocument();
}, 15000);

it("shows only a banner when the SZU session needs auth, without reading the timetable", async () => {
  vi.mocked(invoke).mockImplementation(async (command: string, args?: unknown) => {
    if (command === "signin_get_qw_status") {
      return { state: "needs_auth", message: "请先完成应用统一认证" };
    }
    return defaultInvoke(command, args);
  });
  await mount("/study/signin?tab=szu");
  expect(await screen.findByText("尚未获取到 SZU 考勤会话")).toBeInTheDocument();
  expect(await screen.findByText("请先完成应用统一认证")).toBeInTheDocument();
  expect(screen.queryByText("概率论与数理统计")).not.toBeInTheDocument();
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "signin_get_timetable")).toBe(
    false,
  );
}, 15000);

it("desktop QR section joins the room for matching", async () => {
  await mount("/study/signin");
  // 大学英语是二维码类型点名，模态框内才有匹配区。
  fireEvent.click(await screen.findByText("大学英语"));
  const matchButton = await screen.findByRole("button", { name: "开始匹配共享签到码" });
  expect(matchButton).toBeInTheDocument();
  await act(async () => {
    fireEvent.click(matchButton);
  });
  await waitFor(() => {
    expect(
      vi.mocked(invoke).mock.calls.some(([command]) => command === "signin_qr_room_join"),
    ).toBe(true);
  });
  expect(await screen.findByText("共享签到码匹配中…")).toBeInTheDocument();
}, 15000);

it("opens the rollcall dialog from the open=1 deep link", async () => {
  // 默认搜索解析器把 open=1 解析成数字 1（TanStack JSON.parse），
  // 路由必须同时接受数字；自动打开只触发一次。
  await mount("/study/signin?tab=lms&course=大学英语&open=1");
  expect(await screen.findByRole("button", { name: "开始匹配共享签到码" })).toBeInTheDocument();
}, 15000);

it("top-bar refresh forces a reload of the current tab", async () => {
  await mount("/study/signin");
  await screen.findByText("数据库系统");
  const calls = () =>
    vi.mocked(invoke).mock.calls.filter(([command]) => command === "signin_get_lms_rollcalls");
  expect(calls()).toHaveLength(1);

  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "刷新签到信息" }));
  });
  await waitFor(() => expect(calls().length).toBeGreaterThan(1));
  expect(calls()[1]?.[1]).toEqual({ force: true });
}, 15000);
