/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import DebugLogPage, { copyText } from "./DebugLogPage";

const navigate = vi.fn();
const listeners = new Map<string, (event: { payload: unknown }) => void>();

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("./logical-navigation", () => ({
  useLogicalNavigation: () => ({
    back: () => navigate({ to: "/" }),
    push: (to: string) => navigate({ to }),
  }),
}));

describe("debug log page", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listeners.clear();
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_debug_mode") return { enabled: true, logCount: 1 };
      if (command === "get_debug_logs") {
        return [
          {
            timestampMs: 1_757_400_000_000,
            level: "warn",
            source: "portal",
            stage: "config",
            connected: true,
            transport: "wifi",
            ssid: "S***N",
            ip: "172.30.12.*",
            portal: "staff",
            httpStatus: 503,
            elapsedMs: 123,
            errorKind: "http-status",
            message: "门户返回 HTTP 状态异常",
          },
        ];
      }
      return undefined;
    });
    vi.mocked(listen).mockImplementation(async (event, callback) => {
      listeners.set(event, callback as (event: { payload: unknown }) => void);
      return () => listeners.delete(event);
    });
  });
  afterEach(cleanup);

  it("loads, copies, clears, and disables diagnostics", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    render(<DebugLogPage />);

    expect(await screen.findByText("调试模式已启用")).toBeInTheDocument();
    expect(screen.getByText(/portal\/config/)).toHaveTextContent("S***N");

    fireEvent.click(screen.getByRole("button", { name: /复制全部/ }));
    await waitFor(() =>
      expect(writeText).toHaveBeenCalledWith(expect.stringContaining("portal/config")),
    );
    fireEvent.click(screen.getByRole("button", { name: /清空日志/ }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("clear_debug_logs"));
    expect(await screen.findByText("日志已清空")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /关闭调试模式/ }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_debug_mode", { enabled: false }));
    expect(navigate).toHaveBeenCalledWith({ to: "/" });
  });

  it("uses the selection fallback when clipboard API is unavailable", async () => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: vi.fn().mockRejectedValue(new Error("blocked")) },
    });
    const execCommand = vi.fn().mockReturnValue(true);
    Object.defineProperty(document, "execCommand", { configurable: true, value: execCommand });
    await expect(copyText("safe diagnostic text")).resolves.toBe(true);
    expect(execCommand).toHaveBeenCalledWith("copy");
  });

  it("appends live diagnostic entries while enabled", async () => {
    render(<DebugLogPage />);
    await screen.findByText("调试模式已启用");
    listeners.get("debug-log-added")?.({
      payload: {
        timestampMs: 1_757_400_000_001,
        level: "info",
        source: "network",
        stage: "snapshot",
        connected: true,
        transport: "cellular",
      },
    });
    await waitFor(() => expect(screen.getByText(/network\/snapshot/)).toBeInTheDocument());
  });
});
