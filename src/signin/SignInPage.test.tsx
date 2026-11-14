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

const rollcalls: RollcallItem[] = [
  {
    rollcall_id: 123,
    is_number: true,
    is_radar: false,
    course_id: 9,
    course_title: "数据库系统",
    student_rollcall_id: 1,
    kind: "number",
    status: "on_call",
  },
  {
    rollcall_id: 456,
    is_number: false,
    is_radar: true,
    course_id: 10,
    course_title: "概率论与数理统计",
    student_rollcall_id: 2,
    kind: "radar",
    status: "on_call",
  },
  {
    rollcall_id: 789,
    is_number: false,
    is_radar: false,
    course_id: 11,
    course_title: "大学英语",
    student_rollcall_id: 3,
    kind: "qr",
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

function setupInvoke() {
  submitted = [];
  vi.mocked(invoke).mockImplementation(async (command: string, args?: unknown) => {
    switch (command) {
      case "get_auth_state":
        return { Authenticated: { studentId: "20230001", name: "测试" } };
      case "get_onboarding_state":
        return { steps: [], needsSetup: false, currentStep: null };
      case "signin_get_buildings":
        return buildings;
      case "signin_get_qw_status":
        return { state: "ready", message: "" };
      case "signin_get_lms_rollcalls":
        return rollcalls;
      case "signin_resolve_radar_point":
        return {
          latitude: 22.528,
          longitude: 113.94,
          label: "致理楼",
          source: "fallback",
          fenceRadiusM: 130,
        };
      case "signin_get_timetable":
        return { fetched_at_ms: 1, semester: "2026-2027-1", sessions: [todaySession()] };
      case "signin_get_last_photo":
        return { hasPhoto: false };
      case "signin_submit_lms_number":
      case "signin_submit_lms_radar":
      case "signin_submit_szu":
        submitted.push({ command, args: (args ?? {}) as Record<string, unknown> });
        return command === "signin_submit_lms_radar"
          ? { latitude: 22.528, longitude: 113.94, label: "致理楼", source: "fallback" }
          : null;
      case "signin_submit_lms_qr":
        submitted.push({ command, args: (args ?? {}) as Record<string, unknown> });
        return null;
      default:
        return undefined;
    }
  });
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

  // 点击签到项打开模态框（数字签到）。
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

it("radar modal resolves the default point and submits the resolved coordinates", async () => {
  await mount("/study/signin");
  fireEvent.click(await screen.findByText("概率论与数理统计"));
  // 雷达表单出现：默认点标签与提交按钮。
  expect(await screen.findByTestId("radar-point-label")).toHaveTextContent("致理楼");
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "确认提交" }));
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
  expect(await screen.findByText("今天的课次")).toBeInTheDocument();
  expect(await screen.findByText("概率论与数理统计")).toBeInTheDocument();
  expect(screen.queryByText("待签到项")).not.toBeInTheDocument();
}, 15000);

it("submits SZU signin with the selected building", async () => {
  await mount("/study/signin?tab=szu");
  fireEvent.click(await screen.findByText("概率论与数理统计"));
  // SZU 模态框内的雷达表单（致理楼默认点）。
  const pointLabels = await screen.findAllByTestId("radar-point-label");
  expect(pointLabels[0]).toHaveTextContent("致理楼");
  const submitButtons = screen.getAllByRole("button", { name: "确认提交" });
  await act(async () => {
    fireEvent.click(submitButtons[0]);
  });
  await waitFor(() => {
    expect(submitted.some((entry) => entry.command === "signin_submit_szu")).toBe(true);
  });
  const entry = submitted.find((item) => item.command === "signin_submit_szu")!;
  expect(entry.args.courseName).toBe("概率论与数理统计");
  expect(entry.args.buildingId).toBeUndefined();
}, 15000);

it("desktop QR section joins the room for matching", async () => {
  await mount("/study/signin");
  // 大学英语是二维码类型点名，模态框内才有扫码/匹配区。
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
