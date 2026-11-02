/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { PortNoticeBridge, type PortNotice } from "./PortNoticeBridge";

vi.mock("./logical-navigation", () => ({ isChildDocument: () => false }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

function notice(overrides: Partial<PortNotice> = {}): PortNotice {
  return {
    id: 1,
    kind: "mcp-port-changed",
    service: "mcp",
    previousPort: 53901,
    currentPort: 53902,
    message: "MCP 本地服务原端口 53901 已被占用，已自动切换到 53902 并保存该设置。",
    ...overrides,
  };
}

describe("PortNoticeBridge", () => {
  let eventCallback: ((event: { payload: PortNotice }) => void) | null = null;

  beforeEach(() => {
    vi.clearAllMocks();
    (window as any).__TAURI_INTERNALS__ = {};
    eventCallback = null;
    vi.mocked(listen).mockImplementation(async (eventName: string, cb: any) => {
      if (eventName === "app://port-notice") eventCallback = cb;
      return () => undefined;
    });
    vi.mocked(invoke).mockImplementation(async () => []);
  });

  afterEach(() => {
    cleanup();
    delete (window as any).__TAURI_INTERNALS__;
  });

  async function renderBridge(pending: PortNotice[] = []) {
    vi.mocked(invoke).mockImplementation(async () => pending);
    render(<PortNoticeBridge />);
    await waitFor(() => expect(eventCallback).not.toBeNull());
  }

  async function dialogHost() {
    const element = await screen.findByRole("alertdialog");
    return ((element.getRootNode() as ShadowRoot).host as HTMLElement) ?? element;
  }

  it("shows a queued MCP port-change notice taken from the backend", async () => {
    await renderBridge([notice()]);

    const dialog = await dialogHost();
    expect(within(dialog).getByText("MCP 端口已切换")).toBeInTheDocument();
    expect(within(dialog).getByText(/53902/)).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("take_port_notices");
  });

  it("shows a runtime exhaustion notice delivered by the event", async () => {
    await renderBridge();

    eventCallback!({
      payload: notice({
        id: 2,
        kind: "ports-exhausted",
        previousPort: null,
        currentPort: null,
        message: "端口分配失败：aria2 下载服务在 54001–54100 范围内找不到可用端口。",
      }),
    });

    const dialog = await dialogHost();
    expect(within(dialog).getByText("端口分配失败")).toBeInTheDocument();
    expect(within(dialog).getByText(/aria2 下载服务/)).toBeInTheDocument();
  });

  it("dismisses the current notice and reveals the next queued one", async () => {
    await renderBridge([
      notice({ id: 1 }),
      notice({
        id: 2,
        kind: "ports-exhausted",
        previousPort: null,
        currentPort: null,
        message: "端口分配失败：JXXK 录像下载服务在 54101–54200 范围内找不到可用端口。",
      }),
    ]);

    const first = await dialogHost();
    expect(within(first).getByText("MCP 端口已切换")).toBeInTheDocument();
    fireEvent.click(within(first).getByRole("button", { name: "知道了" }));

    await waitFor(() => expect(screen.getByText("端口分配失败")).toBeInTheDocument());
    const second = await dialogHost();
    expect(within(second).getByText(/JXXK 录像下载服务/)).toBeInTheDocument();
  });

  it("does not show the same notice twice when it arrives from both the queue and the event", async () => {
    const payload = notice({ id: 7 });
    await renderBridge([payload]);

    const dialog = await dialogHost();
    eventCallback!({ payload });

    fireEvent.click(within(dialog).getByRole("button", { name: "知道了" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
  });
});
