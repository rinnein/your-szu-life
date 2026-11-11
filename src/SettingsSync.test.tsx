/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { cleanup, render, screen, waitFor, fireEvent } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import SettingsSync from "./SettingsSync";
import { toast } from "./ui";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const disabledStatus = {
  enabled: false,
  campusAccess: false,
  running: false,
  deviceId: "device-a",
  deviceName: "书桌",
  overlayIp: null,
  apiPort: null,
  error: null,
};

const enabledStatus = {
  ...disabledStatus,
  enabled: true,
  running: true,
  overlayIp: "10.126.126.2",
};

const syncStatus = {
  enabled: true,
  campusAccess: false,
  running: true,
  summary: { peers: 1, applied: 0, pushed: 1, conflicts: 0, bankRows: 2, errors: [] },
  items: [
    { id: "uooc.session", label: "UOOC 登录态", collection: false, state: "synced" },
    { id: "oj.accounts", label: "OJ 账号密码", collection: false, state: "waiting" },
    { id: "uooc.qa_bank", label: "UOOC 题库", collection: true, state: "conflict" },
  ],
};

describe("SettingsSync", () => {
  beforeEach(() => {
    vi.mocked(listen).mockResolvedValue(() => undefined);
    vi.mocked(invoke).mockReset();
  });

  it("shows the disabled state and the empty device list", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_mesh_status") return disabledStatus;
      if (command === "get_sync_status") return syncStatus;
      if (command === "list_mesh_devices") return [];
      return undefined;
    });

    render(<SettingsSync />);

    expect(await screen.findByText("设备同步")).toBeInTheDocument();
    expect(screen.getByText("未启用")).toBeInTheDocument();
    expect(screen.getByText("开启设备同步后，这里会显示同账号的设备。")).toBeInTheDocument();
  });

  it("renders devices returned by the backend", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_mesh_status") return enabledStatus;
      if (command === "get_sync_status") return syncStatus;
      if (command === "list_mesh_devices")
        return [
          {
            deviceId: "device-a",
            name: "书桌",
            platform: "linux",
            appVersion: "0.14.0",
            overlayIp: "10.126.126.2",
            isSelf: true,
            reachable: true,
            latencyMs: null,
            syncEnabled: true,
            campusAccess: false,
            onCampus: false,
            region: "none",
            networkState: "unknown",
            ssid: null,
          },
          {
            deviceId: "device-b",
            name: "手机",
            platform: "android",
            appVersion: "0.14.0",
            overlayIp: "10.126.126.3",
            isSelf: false,
            reachable: true,
            latencyMs: 12,
            syncEnabled: true,
            campusAccess: true,
            onCampus: true,
            region: "dormitory",
            networkState: "connected",
            ssid: "SZU_CTC&CMCC",
          },
        ];
      return undefined;
    });

    render(<SettingsSync />);

    expect(await screen.findByText("书桌")).toBeInTheDocument();
    expect(screen.getByText("手机")).toBeInTheDocument();
    expect(screen.getByText("1 / 1 台在线")).toBeInTheDocument();
  });

  it("enables sync through the command and reports the result", async () => {
    const success = vi.spyOn(toast, "success").mockImplementation(() => undefined as never);
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_mesh_status") return disabledStatus;
      if (command === "get_sync_status") return syncStatus;
      if (command === "list_mesh_devices") return [];
      if (command === "set_mesh_enabled") {
        expect(args).toMatchObject({ enabled: true });
        return enabledStatus;
      }
      return undefined;
    });

    render(<SettingsSync />);
    await screen.findByText("未启用");

    const toggle = screen.getByLabelText("启用设备同步");
    fireEvent.click(toggle);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_mesh_enabled", { enabled: true });
      expect(success).toHaveBeenCalled();
    });
  });

  it("saves the device name", async () => {
    const success = vi.spyOn(toast, "success").mockImplementation(() => undefined as never);
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_mesh_status") return enabledStatus;
      if (command === "get_sync_status") return syncStatus;
      if (command === "list_mesh_devices") return [];
      if (command === "set_mesh_device_name") {
        expect(args).toMatchObject({ name: "新名字" });
        return { ...enabledStatus, deviceName: "新名字" };
      }
      return undefined;
    });

    render(<SettingsSync />);
    await screen.findByText("设备同步");

    const input = screen.getByLabelText("设备名称");
    fireEvent.input(input, { target: { value: "新名字" } });
    fireEvent.click(screen.getByRole("button", { name: /保存/ }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_mesh_device_name", { name: "新名字" });
      expect(success).toHaveBeenCalled();
    });
  });

  it("lists the whitelisted items and marks frozen conflicts", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_mesh_status") return enabledStatus;
      if (command === "get_sync_status") return syncStatus;
      if (command === "list_mesh_devices") return [];
      return undefined;
    });

    render(<SettingsSync />);

    expect(await screen.findByText("UOOC 登录态")).toBeInTheDocument();
    expect(screen.getByText("OJ 账号密码")).toBeInTheDocument();
    expect(screen.getByText("UOOC 题库")).toBeInTheDocument();
    expect(screen.getByText("冲突冻结")).toBeInTheDocument();
    expect(screen.getByText(/有 1 项\/行冲突/)).toBeInTheDocument();
  });
});
