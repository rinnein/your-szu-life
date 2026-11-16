/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
import { NotificationSettings, type NotificationConfig } from "./NotificationSettings";
import { toast } from "./ui";

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
  enrollment: true,
  uooc: true,
  todo: {
    enabled: true,
    discovery: true,
    ojQuiz: true,
    uoocExam: true,
    uoocHomework: true,
    uoocCourseEnd: true,
    lmsHomework: true,
    electricityOutage: true,
    checkin: true,
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
    vi.restoreAllMocks();
  });

  /** 读取完成前所有控件都禁用（防止把默认值写回），交互前先等它可用。 */
  async function readySwitch(name: string) {
    const element = await screen.findByRole("switch", { name });
    await waitFor(() => expect(element).toBeEnabled());
    return element;
  }

  it("renders notification settings with all switches enabled by default", async () => {
    render(<NotificationSettings />);

    const masterSwitch = await screen.findByRole("switch", { name: "启用所有通知" });
    expect(masterSwitch).toBeInTheDocument();
    expect(masterSwitch).toBeChecked();

    expect(screen.getByRole("switch", { name: "开关宿舍校园网通知" })).toBeChecked();
    expect(screen.getByRole("switch", { name: "开关选课助手通知" })).toBeChecked();
    expect(screen.getByRole("switch", { name: "开关优课联盟 (UOOC)通知" })).toBeChecked();
  });

  it("toggling master switch saves disabled state and disables category switches", async () => {
    render(<NotificationSettings />);

    const masterSwitch = await readySwitch("启用所有通知");
    fireEvent.change(masterSwitch, { target: { checked: false } });

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_notification_settings", {
        settings: expect.objectContaining({
          enabled: false,
        }),
      });
    });

    expect(screen.getByRole("switch", { name: "开关宿舍校园网通知" })).toBeDisabled();
  });

  it("toggling individual category switch saves updated category setting", async () => {
    render(<NotificationSettings />);

    const enrollmentSwitch = await readySwitch("开关选课助手通知");
    fireEvent.change(enrollmentSwitch, { target: { checked: false } });

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_notification_settings", {
        settings: expect.objectContaining({
          enabled: true,
          enrollment: false,
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
      "开关宿舍电费预警提醒",
      "开关签到提醒",
    ]) {
      expect(await screen.findByRole("switch", { name })).toBeChecked();
    }

    for (const name of ["7 天前", "3 天前", "1 天前", "2 小时前"]) {
      expect(screen.getByRole("checkbox", { name })).toBeChecked();
    }
    for (const name of ["2 天前", "1 小时前", "30 分钟前", "15 分钟前", "5 分钟前"]) {
      expect(screen.getByRole("checkbox", { name })).not.toBeChecked();
    }

    const discovery = screen.getByRole("checkbox", { name: "初次探测到时" });
    expect(discovery).toBeChecked();
    expect(discovery).toBeEnabled();

    const started = screen.getByRole("checkbox", { name: "任务开始时" });
    expect(started).toBeChecked();
    expect(started).toBeDisabled();
  });

  it("saves per-item todo switches", async () => {
    render(<NotificationSettings />);

    const examSwitch = await readySwitch("开关UOOC 考试提醒");
    fireEvent.change(examSwitch, { target: { checked: false } });

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_notification_settings", {
        settings: expect.objectContaining({
          todo: expect.objectContaining({ enabled: true, uoocExam: false }),
        }),
      });
    });
  });

  it("saves the discovery toggle", async () => {
    render(<NotificationSettings />);

    const discovery = await screen.findByRole("checkbox", { name: "初次探测到时" });
    await waitFor(() => expect(discovery).toBeEnabled());
    fireEvent.change(discovery, { target: { checked: false } });

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_notification_settings", {
        settings: expect.objectContaining({
          todo: expect.objectContaining({ discovery: false }),
        }),
      });
    });
  });

  it("saves selected lead times as a deduplicated descending list", async () => {
    render(<NotificationSettings />);

    const twoHours = await screen.findByRole("checkbox", { name: "2 小时前" });
    await waitFor(() => expect(twoHours).toBeEnabled());
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
    expect(await screen.findByText(/只在「初次探测到时」与「任务开始时」提醒/)).toBeInTheDocument();
  });

  it("disables todo controls when the todo master switch is off", async () => {
    render(<NotificationSettings />);

    const todoMaster = await readySwitch("启用待办提醒");
    fireEvent.change(todoMaster, { target: { checked: false } });

    await waitFor(() => {
      expect(screen.getByRole("switch", { name: "开关OJ 测验提醒" })).toBeDisabled();
    });
    expect(screen.getByRole("checkbox", { name: "7 天前" })).toBeDisabled();

    const discovery = screen.getByRole("checkbox", { name: "初次探测到时" });
    expect(discovery).toBeChecked();
    expect(discovery).toBeDisabled();
  });

  it("shows a retry banner and keeps every control disabled when the read fails", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_notification_settings") throw "读取设置文件失败";
      return undefined;
    });

    render(<NotificationSettings />);

    expect(await screen.findByText("读取通知设置失败")).toBeInTheDocument();
    expect(screen.getByText(/读取设置文件失败/)).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "启用所有通知" })).toBeDisabled();
    expect(screen.getByRole("switch", { name: "开关宿舍校园网通知" })).toBeDisabled();
    expect(screen.getByRole("switch", { name: "启用待办提醒" })).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: "7 天前" })).toBeDisabled();

    fireEvent.click(screen.getByRole("button", { name: "重试" }));

    await waitFor(() =>
      expect(
        vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "get_notification_settings"),
      ).toHaveLength(2),
    );
  });

  it("serializes rapid toggles of one switch and keeps the last value", async () => {
    const saves: NotificationConfig[] = [];
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_notification_settings") return DEFAULT_TEST_SETTINGS;
      if (command === "save_notification_settings") {
        const settings = (args as { settings: NotificationConfig }).settings;
        saves.push(settings);
        if (saves.length === 1) await gate;
        return settings;
      }
      return undefined;
    });

    render(<NotificationSettings />);

    const enrollment = await readySwitch("开关选课助手通知");
    fireEvent.change(enrollment, { target: { checked: false } });
    fireEvent.change(screen.getByRole("switch", { name: "开关选课助手通知" }), {
      target: { checked: true },
    });

    // 第一次保存还没回来时，第二次改动不能另起一个请求。
    await waitFor(() => expect(saves).toHaveLength(1));
    expect(saves[0].enrollment).toBe(false);

    release();

    await waitFor(() => expect(saves).toHaveLength(2));
    expect(saves[1].enrollment).toBe(true);
  });

  it("rolls the toggle back when the save fails", async () => {
    const danger = vi.spyOn(toast, "danger").mockImplementation(() => "toast-id");
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_notification_settings") return DEFAULT_TEST_SETTINGS;
      if (command === "save_notification_settings") throw "设置文件不可写";
      return undefined;
    });

    render(<NotificationSettings />);

    const enrollment = await readySwitch("开关选课助手通知");
    fireEvent.change(enrollment, { target: { checked: false } });

    await waitFor(() => expect(danger).toHaveBeenCalled());
    await waitFor(() =>
      expect(screen.getByRole("switch", { name: "开关选课助手通知" })).toBeChecked(),
    );
  });
});
