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

  it("filters logs by search query and level", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_debug_mode") return { enabled: true, logCount: 2 };
      if (command === "get_debug_logs") {
        return [
          {
            timestampMs: 1_757_400_000_000,
            level: "warn",
            source: "portal",
            stage: "config",
            ssid: "SZU_WIFI_TEST",
            message: "portal warning detail",
          },
          {
            timestampMs: 1_757_400_001_000,
            level: "info",
            source: "network",
            stage: "probe",
            ip: "192.168.1.100",
            message: "network probe success",
          },
        ];
      }
      return undefined;
    });

    render(<DebugLogPage />);
    await screen.findByText("调试模式已启用");

    // Both logs are initially displayed
    expect(screen.getByText(/portal\/config/)).toBeInTheDocument();
    expect(screen.getByText(/network\/probe/)).toBeInTheDocument();

    // Filter by search query "SZU_WIFI"
    const searchInput = screen.getByPlaceholderText(/搜索日志内容/);
    fireEvent.change(searchInput, { target: { value: "SZU_WIFI" } });
    expect(screen.getByText(/portal\/config/)).toBeInTheDocument();
    expect(screen.queryByText(/network\/probe/)).not.toBeInTheDocument();

    // Clear search
    fireEvent.change(searchInput, { target: { value: "" } });
    expect(screen.getByText(/network\/probe/)).toBeInTheDocument();

    // Filter by level "INFO"
    fireEvent.click(screen.getByRole("button", { name: "级别筛选 INFO" }));
    expect(screen.queryByText(/portal\/config/)).not.toBeInTheDocument();
    expect(screen.getByText(/network\/probe/)).toBeInTheDocument();

    // Switch to "ALL"
    fireEvent.click(screen.getByRole("button", { name: "级别筛选 全部" }));
    expect(screen.getByText(/portal\/config/)).toBeInTheDocument();
    expect(screen.getByText(/network\/probe/)).toBeInTheDocument();
  });

  it("toggles between accordion and table views without truncating details", async () => {
    const longMessage = "VeryLongMessageWithoutTruncation_ErrorDetail_1234567890_abcdefghij";
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_debug_mode") return { enabled: true, logCount: 1 };
      if (command === "get_debug_logs") {
        return [
          {
            timestampMs: 1_757_400_000_000,
            level: "error",
            source: "portal",
            stage: "login",
            message: longMessage,
          },
        ];
      }
      return undefined;
    });

    render(<DebugLogPage />);
    await screen.findByText("调试模式已启用");

    // Default table view shows long message without truncation
    expect(screen.getByRole("grid", { name: /诊断日志数据表格/ })).toBeInTheDocument();
    expect(screen.getAllByText(new RegExp(longMessage)).length).toBeGreaterThanOrEqual(1);

    // Toggle to accordion view
    fireEvent.click(screen.getByRole("button", { name: /折叠面板/ }));
    expect(screen.getByText(/全部展开/)).toBeInTheDocument();
    expect(screen.getAllByText(new RegExp(longMessage)).length).toBeGreaterThanOrEqual(1);

    // Toggle back to table view
    fireEvent.click(screen.getByRole("button", { name: /表格视图/ }));
    expect(screen.getByRole("grid", { name: /诊断日志数据表格/ })).toBeInTheDocument();
  });

  it("loads logs in chunks using get_debug_logs_jsonl and manual load more", async () => {
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_debug_mode") return { enabled: true, logCount: 2 };
      if (command === "get_debug_logs_jsonl") {
        const { offset } = (args ?? {}) as { offset?: number };
        if (offset === 0) {
          return {
            lines: [
              JSON.stringify({
                timestampMs: 1_757_400_000_000,
                level: "info",
                source: "jsonl_chunk_1",
                stage: "stage_1",
                message: "first chunk message",
              }),
            ],
            total: 2,
            hasMore: true,
          };
        }
        return {
          lines: [
            JSON.stringify({
              timestampMs: 1_757_400_001_000,
              level: "warn",
              source: "jsonl_chunk_2",
              stage: "stage_2",
              message: "second chunk message",
            }),
          ],
          total: 2,
          hasMore: false,
        };
      }
      return undefined;
    });

    render(<DebugLogPage />);
    await screen.findByText("调试模式已启用");

    // First chunk loads immediately
    expect(await screen.findByText(/jsonl_chunk_1\/stage_1/)).toBeInTheDocument();
    expect(screen.queryByText(/jsonl_chunk_2\/stage_2/)).not.toBeInTheDocument();

    // Click manual load more chunk button
    const loadMoreBtn = screen.getByRole("button", { name: /分片加载更多/ });
    fireEvent.click(loadMoreBtn);

    // Second chunk loads on user demand
    await waitFor(() => expect(screen.getByText(/jsonl_chunk_2\/stage_2/)).toBeInTheDocument());
  });

  it("automatically loads next chunk when switching to next page", async () => {
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_debug_mode") return { enabled: true, logCount: 40 };
      if (command === "get_debug_logs_jsonl") {
        const { offset } = (args ?? {}) as { offset?: number };
        if (offset === 0) {
          return {
            lines: Array.from({ length: 20 }, (_, i) =>
              JSON.stringify({
                timestampMs: 1_757_400_000_000 + i * 1000,
                level: "info",
                source: "page1",
                stage: `item_${i}`,
                message: `page1 item ${i}`,
              }),
            ),
            total: 40,
            hasMore: true,
          };
        }
        return {
          lines: Array.from({ length: 20 }, (_, i) =>
            JSON.stringify({
              timestampMs: 1_757_400_100_000 + i * 1000,
              level: "warn",
              source: "page2",
              stage: `item_${i}`,
              message: `page2 item ${i}`,
            }),
          ),
          total: 40,
          hasMore: false,
        };
      }
      return undefined;
    });

    render(<DebugLogPage />);
    await screen.findByText("调试模式已启用");

    // Change page size to 20 so page 1 is full with chunk 1
    fireEvent.click(screen.getByRole("button", { name: "20" }));

    // Page 1 items are shown
    expect(await screen.findByText(/page1\/item_0/)).toBeInTheDocument();
    expect(screen.queryByText(/page2\/item_0/)).not.toBeInTheDocument();

    // Click "下一页" button
    const nextBtn = screen.getByRole("button", { name: /下一页/ });
    fireEvent.click(nextBtn);

    // Auto-loads chunk 2 and shows page 2 items
    await waitFor(() => expect(screen.getByText(/page2\/item_0/)).toBeInTheDocument());
    expect(screen.queryByText(/page1\/item_0/)).not.toBeInTheDocument();

    // Click "上一页" button to return to page 1
    const prevBtn = screen.getByRole("button", { name: /上一页/ });
    fireEvent.click(prevBtn);

    // Page 1 items are shown again without issue
    await waitFor(() => expect(screen.getByText(/page1\/item_0/)).toBeInTheDocument());
    expect(screen.queryByText(/page2\/item_0/)).not.toBeInTheDocument();
  });

  it("automatically prefetches next chunk in background when navigating to last loaded page", async () => {
    let chunk2Requested = false;
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_debug_mode") return { enabled: true, logCount: 50 };
      if (command === "get_debug_logs_jsonl") {
        const { offset } = (args ?? {}) as { offset?: number };
        if (offset === 0) {
          return {
            lines: Array.from({ length: 40 }, (_, i) =>
              JSON.stringify({
                timestampMs: 1_757_400_000_000 + i * 1000,
                level: "info",
                source: "source_A",
                stage: `step_${i}`,
                message: `msg ${i}`,
              }),
            ),
            total: 60,
            hasMore: true,
          };
        }
        chunk2Requested = true;
        return {
          lines: Array.from({ length: 20 }, (_, i) =>
            JSON.stringify({
              timestampMs: 1_757_400_200_000 + i * 1000,
              level: "error",
              source: "source_B",
              stage: `step_${i}`,
              message: `msg ${i}`,
            }),
          ),
          total: 60,
          hasMore: false,
        };
      }
      return undefined;
    });

    render(<DebugLogPage />);
    await screen.findByText("调试模式已启用");

    // Page size 20: 40 loaded entries span 2 pages (page 1 and page 2)
    fireEvent.click(screen.getByRole("button", { name: "20" }));
    expect(await screen.findByText(/source_A\/step_0/)).toBeInTheDocument();
    expect(chunk2Requested).toBe(false);

    // Navigating to page 2 (the last loaded page) triggers background prefetch of chunk 2
    const nextBtn = screen.getByRole("button", { name: /下一页/ });
    fireEvent.click(nextBtn);

    await waitFor(() => expect(chunk2Requested).toBe(true));
  });
});
