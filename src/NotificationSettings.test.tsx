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
});
