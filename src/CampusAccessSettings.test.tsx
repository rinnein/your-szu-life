/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { afterEach, describe, expect, it, vi } from "vite-plus/test";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { CampusAccessSettings } from "./CampusAccessSettings";
import { toast } from "./ui";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const status = {
  enabled: true,
  campusAccess: true,
  running: true,
  deviceId: "a",
  deviceName: "书桌",
  overlayIp: "10.126.126.2",
  campusEgress: "手机",
  apiPort: 54201,
  error: null,
};

const devices = [
  {
    deviceId: "a",
    name: "书桌",
    platform: "linux",
    overlayIp: "10.126.126.2",
    isSelf: true,
    reachable: true,
    campusAccess: true,
    onCampus: false,
    region: "none",
    networkState: "unknown",
    ssid: null,
  },
  {
    deviceId: "b",
    name: "手机",
    platform: "android",
    overlayIp: "10.126.126.3",
    isSelf: false,
    reachable: true,
    campusAccess: true,
    onCampus: true,
    region: "dormitory",
    networkState: "connected",
    ssid: "SZU_CTC&CMCC",
  },
];

describe("CampusAccessSettings", () => {
  it("shows the current egress and campus-aware device list", async () => {
    vi.mocked(listen).mockResolvedValue(() => undefined);
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_mesh_status") return status;
      if (command === "list_mesh_devices") return devices;
      return undefined;
    });

    render(<CampusAccessSettings />);

    expect(await screen.findByText("校园网访问")).toBeInTheDocument();
    expect(await screen.findByText("可用出口：手机")).toBeInTheDocument();
    expect(screen.getByText(/SZU_CTC&CMCC/)).toBeInTheDocument();
    expect(screen.queryByText("没有可用的校园网出口")).not.toBeInTheDocument();
  });

  it("warns when there is no campus device to borrow", async () => {
    vi.mocked(listen).mockResolvedValue(() => undefined);
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_mesh_status") return { ...status, campusEgress: null };
      if (command === "list_mesh_devices") return [devices[0]];
      return undefined;
    });

    render(<CampusAccessSettings />);

    expect(await screen.findByText("没有可用的校园网出口")).toBeInTheDocument();
  });

  it("toggles the feature through the command", async () => {
    const success = vi.spyOn(toast, "success").mockImplementation(() => undefined as never);
    vi.mocked(listen).mockResolvedValue(() => undefined);
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_mesh_status") return status;
      if (command === "list_mesh_devices") return devices;
      if (command === "set_mesh_campus_access") {
        expect(args).toMatchObject({ enabled: false });
        return { ...status, campusAccess: false };
      }
      return undefined;
    });

    render(<CampusAccessSettings />);
    await screen.findByText("校园网访问");

    fireEvent.click(screen.getByLabelText("启用校园网访问"));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_mesh_campus_access", { enabled: false });
      expect(success).toHaveBeenCalled();
    });
  });
});
