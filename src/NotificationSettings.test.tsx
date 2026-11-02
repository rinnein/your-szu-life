/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
import { NotificationSettings, type NotificationConfig } from "./NotificationSettings";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn(),
  requestPermission: vi.fn(),
}));

const DEFAULT_TEST_SETTINGS: NotificationConfig = {
  enabled: true,
  campusNetwork: true,
  electricity: true,
  enrollment: true,
  oj: true,
  uooc: true,
  todo: {
    enabled: true,
    ojQuiz: true,
    uoocExam: true,
    uoocHomework: true,
    uoocCourseEnd: true,
    lmsHomework: true,
    electricityOutage: true,
    leadMinutes: [10080, 4320, 1440, 120],
  },
};

describe("NotificationSettings component", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "get_notification_settings") {
        return DEFAULT_TEST_SETTINGS;
      }
      if (cmd === "save_notification_settings") {
        return args?.settings;
      }
      return undefined;
    });
    vi.mocked(isPermissionGranted).mockResolvedValue(true);
    vi.mocked(requestPermission).mockResolvedValue("granted");
  });

  afterEach(() => {
    cleanup();
  });

  it("renders notification settings with all switches enabled by default", async () => {
    render(<NotificationSettings />);

    const masterSwitch = await screen.findByRole("switch", { name: "启用所有通知" });
    expect(masterSwitch).toBeInTheDocument();
    expect(masterSwitch).toBeChecked();

    expect(screen.getByRole("switch", { name: "开关宿舍校园网通知" })).toBeChecked();
    expect(screen.getByRole("switch", { name: "开关宿舍电费预警通知" })).toBeChecked();
    expect(screen.getByRole("switch", { name: "开关选课助手通知" })).toBeChecked();
    expect(screen.getByRole("switch", { name: "开关在线评测 (OJ)通知" })).toBeChecked();
    expect(screen.getByRole("switch", { name: "开关优课联盟 (UOOC)通知" })).toBeChecked();
  });

  it("toggling master switch saves disabled state and disables category switches", async () => {
    render(<NotificationSettings />);

    const masterSwitch = await screen.findByRole("switch", { name: "启用所有通知" });
    fireEvent.change(masterSwitch, { target: { checked: false } });

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_notification_settings", {
        settings: expect.objectContaining({
          enabled: false,
        }),
      });
    });

    expect(screen.getByRole("switch", { name: "开关宿舍校园网通知" })).toBeDisabled();
    expect(screen.getByRole("switch", { name: "开关宿舍电费预警通知" })).toBeDisabled();
  });

  it("toggling individual category switch saves updated category setting", async () => {
    render(<NotificationSettings />);

    const electricitySwitch = await screen.findByRole("switch", { name: "开关宿舍电费预警通知" });
    fireEvent.change(electricitySwitch, { target: { checked: false } });

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_notification_settings", {
        settings: expect.objectContaining({
          enabled: true,
          electricity: false,
        }),
      });
    });
  });

  it("displays warning banner when system notification permission is denied", async () => {
    vi.mocked(isPermissionGranted).mockResolvedValue(false);

    render(<NotificationSettings />);

    await waitFor(() => {
      expect(screen.getByText(/系统通知权限未开启/)).toBeInTheDocument();
    });

    const authButton = screen.getByRole("button", { name: "授权系统通知" });
    fireEvent.click(authButton);

    await waitFor(() => {
      expect(requestPermission).toHaveBeenCalled();
    });
  });

  it("renders all lead-time presets with the documented defaults selected", async () => {
    render(<NotificationSettings />);

    for (const name of [
      "开关OJ 测验提醒",
      "开关UOOC 考试提醒",
      "开关UOOC 作业提醒",
      "开关UOOC 结课提醒",
      "开关畅课作业提醒",
      "开关电费预估断电提醒",
    ]) {
      expect(await screen.findByRole("switch", { name })).toBeChecked();
    }

    for (const name of ["7 天前", "3 天前", "1 天前", "2 小时前"]) {
      expect(screen.getByRole("checkbox", { name })).toBeChecked();
    }
    for (const name of ["2 天前", "1 小时前", "30 分钟前", "15 分钟前", "5 分钟前"]) {
      expect(screen.getByRole("checkbox", { name })).not.toBeChecked();
    }
  });

  it("saves per-item todo switches", async () => {
    render(<NotificationSettings />);

    const examSwitch = await screen.findByRole("switch", { name: "开关UOOC 考试提醒" });
    fireEvent.change(examSwitch, { target: { checked: false } });

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_notification_settings", {
        settings: expect.objectContaining({
          todo: expect.objectContaining({ enabled: true, uoocExam: false }),
        }),
      });
    });
  });

  it("saves selected lead times as a deduplicated descending list", async () => {
    render(<NotificationSettings />);

    const twoHours = await screen.findByRole("checkbox", { name: "2 小时前" });
    fireEvent.change(twoHours, { target: { checked: false } });

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_notification_settings", {
        settings: expect.objectContaining({
          todo: expect.objectContaining({
            leadMinutes: [10080, 4320, 1440],
          }),
        }),
      });
    });
  });

  it("warns when no lead time is selected", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "get_notification_settings") {
        return {
          ...DEFAULT_TEST_SETTINGS,
          todo: { ...DEFAULT_TEST_SETTINGS.todo, leadMinutes: [] },
        };
      }
      if (cmd === "save_notification_settings") return args?.settings;
      return undefined;
    });

    render(<NotificationSettings />);
    expect(await screen.findByText(/待办不会发送截止提醒/)).toBeInTheDocument();
  });

  it("disables todo controls when the todo master switch is off", async () => {
    render(<NotificationSettings />);

    const todoMaster = await screen.findByRole("switch", { name: "启用待办提醒" });
    fireEvent.change(todoMaster, { target: { checked: false } });

    await waitFor(() => {
      expect(screen.getByRole("switch", { name: "开关OJ 测验提醒" })).toBeDisabled();
    });
    expect(screen.getByRole("checkbox", { name: "7 天前" })).toBeDisabled();
  });
});
