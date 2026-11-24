/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { PermissionSettings } from "./PermissionSettings";
import { resetToasts } from "./ui";
import type { PermissionKey, PermissionState } from "./permissions";
import type { StepNextAction } from "./step-next-action";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

function makeStates(): PermissionState[] {
  return [
    { id: "notifications", status: "granted", canRequest: false },
    { id: "promotedNotifications", status: "unsupported", canRequest: false },
    { id: "location", status: "systemDisabled", canRequest: false },
    { id: "nearbyWifi", status: "denied", canRequest: true },
    { id: "camera", status: "granted", canRequest: false },
    { id: "installPackages", status: "denied", canRequest: false },
    { id: "exactAlarm", status: "granted", canRequest: false },
    { id: "autoStart", status: "manual", canRequest: false },
    { id: "batteryOptimization", status: "denied", canRequest: false },
  ];
}

let currentStates: PermissionState[] = [];

describe("PermissionSettings", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetToasts();
    currentStates = makeStates();
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_permission_states") return currentStates;
      if (command === "request_permissions") {
        const { keys } = args as { keys: PermissionKey[] };
        currentStates = currentStates.map((item) =>
          keys.includes(item.id) ? { ...item, status: "granted", canRequest: false } : item,
        );
        return currentStates;
      }
      return undefined;
    });
  });

  afterEach(() => {
    cleanup();
    resetToasts();
  });

  it("lists every permission with its purpose and current status", async () => {
    render(<PermissionSettings />);

    expect(await screen.findByText("通知")).toBeInTheDocument();
    expect(screen.getByText("实时活动（流体云）")).toBeInTheDocument();
    expect(screen.getByText("位置")).toBeInTheDocument();
    expect(screen.getByText("附近的设备（Wi-Fi）")).toBeInTheDocument();
    expect(screen.getByText("相机")).toBeInTheDocument();
    expect(screen.getByText("安装未知应用")).toBeInTheDocument();
    expect(screen.getByText("精确闹钟")).toBeInTheDocument();
    // 可选的两项后台引导：自启动无法检测、电池优化可检测。
    expect(screen.getByText("后台自启动")).toBeInTheDocument();
    expect(screen.getByText("电池优化白名单")).toBeInTheDocument();

    // 主标题下的用途说明与权限名都展示给用户。
    expect(screen.getByText("POST_NOTIFICATIONS")).toBeInTheDocument();
    expect(screen.getByText("SCHEDULE_EXACT_ALARM")).toBeInTheDocument();
    expect(screen.getByText("REQUEST_IGNORE_BATTERY_OPTIMIZATIONS")).toBeInTheDocument();
    expect(screen.getAllByText("安装时授予")).toHaveLength(3);

    expect(screen.getByText("系统不支持")).toBeInTheDocument();
    expect(screen.getByText("系统关闭")).toBeInTheDocument();
    expect(screen.getByText("需手动确认")).toBeInTheDocument();
    // 未授予：附近的设备、安装未知应用、电池优化。
    expect(screen.getAllByText("未授予")).toHaveLength(3);
    expect(screen.getByRole("button", { name: /^授权/ })).toBeInTheDocument();
    // 去设置：位置（系统关闭）、安装未知应用、后台自启动、电池优化。
    expect(screen.getAllByRole("button", { name: /^去设置/ })).toHaveLength(4);
  });

  it("requests a runtime permission and applies the returned state", async () => {
    render(<PermissionSettings />);
    fireEvent.click(await screen.findByRole("button", { name: /^授权/ }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("request_permissions", {
        keys: ["nearbyWifi"],
      }),
    );
    await waitFor(() => expect(screen.getAllByText("未授予")).toHaveLength(2));
  });

  it("opens the related system settings when the permission cannot be requested", async () => {
    render(<PermissionSettings />);
    const settingsButtons = await screen.findAllByRole("button", {
      name: /^去设置/,
    });
    // 行顺序：位置（系统关闭）在前，安装未知应用在后。
    fireEvent.click(settingsButtons[1]);

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("open_permission_settings", {
        target: "installPackages",
      }),
    );
  });

  it("registers a batch action for the onboarding footer", async () => {
    const onComplete = vi.fn();
    const actions: Array<StepNextAction | null> = [];
    render(
      <PermissionSettings
        onboarding
        onComplete={onComplete}
        registerNext={(action) => actions.push(action)}
      />,
    );

    await screen.findByText("附近的设备（Wi-Fi）");
    await waitFor(() => expect(actions[actions.length - 1]).not.toBeNull());

    const action = actions[actions.length - 1];
    if (!action) throw new Error("向导主操作未注册");
    expect(action.label).toBe("授权全部并继续");

    await act(async () => {
      await action.run();
    });

    expect(invoke).toHaveBeenCalledWith("request_permissions", {
      keys: ["nearbyWifi"],
    });
    expect(onComplete).toHaveBeenCalledTimes(1);
  });

  it("keeps the page usable and offers a retry when the status read fails", async () => {
    vi.mocked(invoke).mockRejectedValue(new Error("桥接不可用"));
    render(<PermissionSettings />);

    expect(await screen.findByText("读取系统权限失败")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "重试" }).length).toBeGreaterThan(0);
  });
});
