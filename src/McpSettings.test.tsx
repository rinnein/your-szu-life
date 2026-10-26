/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "./ui";
import McpSettings from "./McpSettings";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("McpSettings component", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_mcp_server_status") {
        return {
          running: true,
          enabled: true,
          port: 53901,
          endpoint: "http://127.0.0.1:53901/mcp",
        };
      }
      return undefined;
    });

    Object.assign(navigator, {
      clipboard: {
        writeText: vi.fn().mockResolvedValue(undefined),
      },
    });
  });

  afterEach(() => {
    cleanup();
  });

  it("renders MCP service status, endpoint URL, and prompt cards", async () => {
    render(<McpSettings />);

    expect(await screen.findByText("本地 MCP 服务状态")).toBeInTheDocument();
    expect(screen.getByText("端口已开启 (53901)")).toBeInTheDocument();
    expect(screen.getByText("http://127.0.0.1:53901/mcp")).toBeInTheDocument();
    expect(screen.getByText("独立高位端口: 53901")).toBeInTheDocument();

    expect(screen.getByText("指导 AI Agent 安装该 MCP 服务")).toBeInTheDocument();
    expect(screen.getByText("配置文件片段 (Claude Desktop / Cursor)")).toBeInTheDocument();
  });

  it("copies endpoint URL and triggers toast", async () => {
    const successSpy = vi.spyOn(toast, "success");
    render(<McpSettings />);

    const copyBtn = await screen.findByRole("button", { name: /复制 URL/i });
    fireEvent.click(copyBtn);

    await waitFor(() => {
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith("http://127.0.0.1:53901/mcp");
      expect(successSpy).toHaveBeenCalledWith("MCP 服务端点 URL 已复制到剪贴板");
    });
  });

  it("copies agent installation prompt and triggers toast", async () => {
    const successSpy = vi.spyOn(toast, "success");
    render(<McpSettings />);

    const copyPromptBtn = await screen.findByRole("button", { name: /复制安装提示词/i });
    fireEvent.click(copyPromptBtn);

    await waitFor(() => {
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
        expect.stringContaining("请帮我连接并配置泥深（your-szu-life）的本地公开 MCP 服务"),
      );
      expect(successSpy).toHaveBeenCalledWith("指导 Agent 安装的提示词已复制到剪贴板");
    });
  });

  it("copies JSON configuration snippet and triggers toast", async () => {
    const successSpy = vi.spyOn(toast, "success");
    render(<McpSettings />);

    const copyJsonBtn = await screen.findByRole("button", { name: /复制 JSON/i });
    fireEvent.click(copyJsonBtn);

    await waitFor(() => {
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
        expect.stringContaining('"your-szu-life"'),
      );
      expect(successSpy).toHaveBeenCalledWith("mcpServers JSON 配置已复制到剪贴板");
    });
  });

  it("toggles MCP server port switch and updates status", async () => {
    const successSpy = vi.spyOn(toast, "success");
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_mcp_server_status") {
        return {
          running: true,
          enabled: true,
          port: 53901,
          endpoint: "http://127.0.0.1:53901/mcp",
        };
      }
      if (command === "set_mcp_server_enabled") {
        const enabled = (args as { enabled: boolean }).enabled;
        return {
          running: enabled,
          enabled,
          port: enabled ? 53901 : null,
          endpoint: enabled ? "http://127.0.0.1:53901/mcp" : null,
        };
      }
      return undefined;
    });

    render(<McpSettings />);

    const switchEl = await screen.findByRole("switch", { name: /管理本地 MCP 服务端口开启状态/i });
    expect(switchEl).toBeChecked();

    fireEvent.click(switchEl);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_mcp_server_enabled", { enabled: false });
      expect(successSpy).toHaveBeenCalledWith("MCP 本地服务已停止，端口已关闭");
    });
  });

  it("refreshes status on button click", async () => {
    render(<McpSettings />);

    await screen.findByText("本地 MCP 服务状态");
    const refreshBtn = screen.getByRole("button", { name: /刷新状态/i });
    fireEvent.click(refreshBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("get_mcp_server_status");
    });
  });
});
