/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import DebugLogPage from "./DebugLogPage";

const navigate = vi.fn();
const listeners = new Map<string, (event: { payload: unknown }) => void>();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class MockChannel {
    onmessage = null;
  },
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openPath: vi.fn().mockResolvedValue(undefined),
  revealItemInDir: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

vi.mock("./logical-navigation", () => ({
  useLogicalNavigation: () => ({
    back: () => navigate({ to: "/" }),
    push: (to: string) => navigate({ to }),
  }),
}));

const mockLogInfo = {
  todayLogPath: "/tmp/app_logs/your-szu-life_2026-10-25.log",
  logDir: "/tmp/app_logs",
  totalSizeBytes: 2048,
  fileCount: 2,
  currentLevel: "INFO",
  availableLevels: ["DEBUG", "INFO", "WARN", "ERROR"],
  files: [
    {
      name: "your-szu-life_2026-10-25.log",
      sizeBytes: 1024,
      modifiedMs: 1_757_400_000_000,
    },
    {
      name: "your-szu-life_2026-10-24.log",
      sizeBytes: 1024,
      modifiedMs: 1_757_313_600_000,
    },
  ],
};

describe("debug log page", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listeners.clear();
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_debug_mode") return { enabled: true, logCount: 0 };
      if (command === "get_log_info") return mockLogInfo;
      if (command === "set_log_level") {
        const { level } = (args ?? {}) as { level: string };
        return level;
      }
      if (command === "clear_debug_logs") return undefined;
      if (command === "set_debug_mode") return { enabled: false, logCount: 0 };
      return undefined;
    });
    vi.mocked(listen).mockImplementation(async (event, callback) => {
      listeners.set(event, callback as (event: { payload: unknown }) => void);
      return () => listeners.delete(event);
    });
  });
  afterEach(cleanup);

  it("renders disabled state when debug mode is off", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_debug_mode") return { enabled: false, logCount: 0 };
      return undefined;
    });

    render(<DebugLogPage />);
    expect(await screen.findByText("调试模式未启用")).toBeInTheDocument();
    expect(screen.getByText("在首页连续点击左上角应用图标 5 次即可启用")).toBeInTheDocument();
    // 作为设置页 tab 呈现，不自行跳转。
    expect(navigate).not.toHaveBeenCalled();
  });

  it("loads log overview and enables quick opening via opener plugin", async () => {
    render(<DebugLogPage />);

    expect(await screen.findByText("日志文件概览")).toBeInTheDocument();
    expect(screen.getByText("/tmp/app_logs/your-szu-life_2026-10-25.log")).toBeInTheDocument();
    expect(screen.getByText("/tmp/app_logs")).toBeInTheDocument();
    expect(screen.getByText("2 个历史文件")).toBeInTheDocument();
    expect(screen.getByText("总大小 2.0 KB")).toBeInTheDocument();

    // Click "打开今日日志"
    fireEvent.click(screen.getByRole("button", { name: /打开今日日志/ }));
    await waitFor(() => {
      expect(openPath).toHaveBeenCalledWith("/tmp/app_logs/your-szu-life_2026-10-25.log");
    });

    // Click "在文件管理器中显示"
    fireEvent.click(screen.getByRole("button", { name: /在文件管理器中显示/ }));
    await waitFor(() => {
      expect(revealItemInDir).toHaveBeenCalledWith("/tmp/app_logs/your-szu-life_2026-10-25.log");
    });
  });

  it("switches log level and triggers backend update", async () => {
    render(<DebugLogPage />);
    await screen.findByText("日志文件概览");

    // Click "DEBUG" level button
    const debugBtn = screen.getByRole("button", { name: /DEBUG/ });
    fireEvent.click(debugBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_log_level", { level: "DEBUG" });
    });
  });

  it("clears logs and refreshes log info", async () => {
    render(<DebugLogPage />);
    await screen.findByText("日志文件概览");

    // Click "清理日志"
    const clearBtn = screen.getByRole("button", { name: /清理日志/ });
    fireEvent.click(clearBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("clear_debug_logs");
    });
  });

  it("disables debug mode without leaving the settings page", async () => {
    render(<DebugLogPage />);
    await screen.findByText("日志文件概览");

    // Click "关闭调试模式"
    const disableBtn = screen.getByRole("button", { name: /关闭调试模式/ });
    fireEvent.click(disableBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_debug_mode", { enabled: false });
    });
    expect(navigate).not.toHaveBeenCalled();
  });
});
