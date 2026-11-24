/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { BackgroundSettings, type StartupSettings } from "./BackgroundSettings";
import { resetToasts } from "./ui";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const DEFAULT_SETTINGS: StartupSettings = {
  autostart: true,
  silent: false,
  supported: true,
};

describe("BackgroundSettings", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetToasts();
    vi.mocked(invoke).mockImplementation(async (command: string, args?: any) => {
      if (command === "get_startup_settings") return DEFAULT_SETTINGS;
      if (command === "save_startup_settings") return args?.settings;
      return undefined;
    });
  });

  afterEach(() => {
    cleanup();
    resetToasts();
  });

  /** 读取完成前控件禁用，交互前先等它可用。 */
  async function readySwitch(name: string) {
    const element = await screen.findByRole("switch", { name });
    await waitFor(() => expect(element).toBeEnabled());
    return element;
  }

  it("shows autostart on and silent off by default", async () => {
    render(<BackgroundSettings />);

    expect(await readySwitch("开机自启")).toBeChecked();
    expect(screen.getByRole("switch", { name: "静默启动" })).not.toBeChecked();
  });

  it("persists the autostart switch", async () => {
    render(<BackgroundSettings />);

    const autostart = await readySwitch("开机自启");
    fireEvent.change(autostart, { target: { checked: false } });

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_startup_settings", {
        settings: { autostart: false, silent: false, supported: true },
      }),
    );
    await waitFor(() => expect(screen.getByRole("switch", { name: "开机自启" })).not.toBeChecked());
  });

  it("disables silent start while autostart is off", async () => {
    render(<BackgroundSettings />);

    const autostart = await readySwitch("开机自启");
    fireEvent.change(autostart, { target: { checked: false } });

    await waitFor(() => expect(screen.getByRole("switch", { name: "静默启动" })).toBeDisabled());
  });

  it("persists the silent switch", async () => {
    render(<BackgroundSettings />);

    const silent = await readySwitch("静默启动");
    fireEvent.change(silent, { target: { checked: true } });

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_startup_settings", {
        settings: { autostart: true, silent: true, supported: true },
      }),
    );
  });

  it("rolls back the switch when saving fails", async () => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === "get_startup_settings") return DEFAULT_SETTINGS;
      throw new Error("写入系统自启项失败");
    });
    render(<BackgroundSettings />);

    const autostart = await readySwitch("开机自启");
    fireEvent.change(autostart, { target: { checked: false } });

    await waitFor(() => expect(screen.getByRole("switch", { name: "开机自启" })).toBeChecked());
  });

  it("explains dev and mobile builds and disables switches when unsupported", async () => {
    vi.mocked(invoke).mockImplementation(async (command: string) =>
      command === "get_startup_settings" ? { ...DEFAULT_SETTINGS, supported: false } : undefined,
    );
    render(<BackgroundSettings />);

    expect(await screen.findByText("当前构建不管理开机自启")).toBeInTheDocument();
    expect(await screen.findByRole("switch", { name: "开机自启" })).toBeDisabled();
    expect(screen.getByRole("switch", { name: "静默启动" })).toBeDisabled();
  });

  it("disables controls and offers retry when the read fails", async () => {
    vi.mocked(invoke).mockRejectedValue(new Error("读取失败"));
    render(<BackgroundSettings />);

    expect(await screen.findByText("读取后台设置失败")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重试" })).toBeInTheDocument();
  });
});
