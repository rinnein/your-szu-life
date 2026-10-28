/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { createMemoryHistory, createRouter, RouterProvider } from "@tanstack/react-router";
import { routeTree } from "./routeTree.gen";
import { listen } from "@tauri-apps/api/event";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

function renderRouterApp(initialEntry = "/settings") {
  const history = createMemoryHistory({
    initialEntries: ["/", initialEntry],
  });
  const testRouter = createRouter({ routeTree, history });
  return { ...render(<RouterProvider router={testRouter} />), testRouter };
}

describe("SettingsPage multi-tab and unified local model card", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        unobserve() {}
        disconnect() {}
      },
    );
    Element.prototype.getAnimations = () => [];
    Element.prototype.scrollIntoView = vi.fn();

    vi.mocked(listen).mockResolvedValue(() => undefined);
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_auth_state")
        return { Authenticated: { studentId: "1234567890", name: "测试同学" } };
      if (command === "get_account_profile")
        return {
          studentId: "1234567890",
          campusCardNumber: "123456",
          hasPassword: true,
          name: "测试同学",
        };
      if (command === "get_user_data") return { studentId: "1234567890", name: "测试同学" };
      if (command === "get_ocr_model_status") {
        return {
          status: "ready",
          revision: "rev",
          sha256: "33b5cd351ee94e73a6bf8fa18c415ed8b819b3ffd342e267c30d8ad8334e34e8",
          bytes: 2294200,
          source: "cache",
        };
      }
      if (command === "get_embedding_model_status") {
        return {
          status: "ready",
          revision: "rev",
          sha256: "15b717c382bcb518ba457b93ea6850ede7f4f1cd8937454aa06972366cd19bcc",
          bytes: 24010842,
          source: "cache",
        };
      }
      if (command === "get_uooc_ai_connections") {
        return {
          main: { baseUrl: "https://example.org/v1", hasKey: true },
          jev: { baseUrl: "https://api.typesafe.ai/v1", hasKey: false },
        };
      }
      if (command === "get_proxy_settings") {
        return {
          mode: "auto",
          protocol: "http",
          host: "",
          port: 8080,
          username: "",
          hasPassword: false,
          campusThroughProxy: false,
        };
      }
      if (command === "get_mcp_server_status") {
        return {
          running: true,
          port: 53901,
          endpoint: "http://127.0.0.1:53901/mcp",
        };
      }
      return undefined;
    });
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("renders the settings tabs and defaults to 个人信息 without privacy storage card", async () => {
    renderRouterApp("/settings");

    expect(await screen.findByRole("tab", { name: /个人信息/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /外观/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /通知/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /网络/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /本地模型/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /AI配置/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /MCP/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /设置迁移/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /快捷键/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /关于/ })).toBeInTheDocument();
    // 诊断日志是开发者 tab，只在调试模式启用后出现。
    expect(screen.queryByRole("tab", { name: /诊断日志/ })).not.toBeInTheDocument();

    // Default tab shows ProfileForm
    expect(screen.getByText("确认账户资料")).toBeInTheDocument();

    // Privacy storage card must NOT be present
    expect(screen.queryByText("隐私信息存储")).not.toBeInTheDocument();
    expect(screen.queryByText("当前使用系统密钥环")).not.toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("get_credential_storage_status");
  });

  it("switches to 网络 tab and displays NetworkSettings", async () => {
    const { testRouter } = renderRouterApp("/settings");

    await screen.findByRole("tab", { name: /网络/ });
    fireEvent.click(screen.getByRole("tab", { name: /网络/ }));

    await waitFor(() => {
      expect(testRouter.state.location.search).toMatchObject({ tab: "network" });
    });

    expect(await screen.findByText("代理模式")).toBeInTheDocument();
    expect(screen.getByText("校内网也走代理")).toBeInTheDocument();
  });

  it("switches to 通知 tab and displays NotificationSettings", async () => {
    const { testRouter } = renderRouterApp("/settings");

    await screen.findByRole("tab", { name: /通知/ });
    fireEvent.click(screen.getByRole("tab", { name: /通知/ }));

    await waitFor(() => {
      expect(testRouter.state.location.search).toMatchObject({ tab: "notifications" });
    });

    expect(await screen.findByText("通知分类管理")).toBeInTheDocument();
  });

  it("switches to 本地模型 tab and displays unified models management", async () => {
    const { testRouter } = renderRouterApp("/settings");

    await screen.findByRole("tab", { name: /本地模型/ });
    fireEvent.click(screen.getByRole("tab", { name: /本地模型/ }));

    await waitFor(() => {
      expect(testRouter.state.location.search).toMatchObject({ tab: "models" });
    });

    expect(await screen.findByText("本地离线模型")).toBeInTheDocument();
    expect(screen.getByText("ddddocr")).toBeInTheDocument();
    expect(screen.getByText("BGE-small-zh-v1.5 (量化版)")).toBeInTheDocument();
    expect(screen.getByText("选课系统与在线评测（OJ）验证码离线识别")).toBeInTheDocument();
    expect(screen.getByText("UOOC 题库检索与语义相似度计算")).toBeInTheDocument();
  });

  it("switches to AI配置 tab and displays AiConnectionSettings", async () => {
    const { testRouter } = renderRouterApp("/settings");

    await screen.findByRole("tab", { name: /AI配置/ });
    fireEvent.click(screen.getByRole("tab", { name: /AI配置/ }));

    await waitFor(() => {
      expect(testRouter.state.location.search).toMatchObject({ tab: "ai" });
    });

    expect(await screen.findByText("AI 连接")).toBeInTheDocument();
    expect(screen.getByLabelText("主模型 API 地址")).toBeInTheDocument();
  });

  it("switches to MCP tab and displays MCP endpoint and prompt", async () => {
    const { testRouter } = renderRouterApp("/settings");

    await screen.findByRole("tab", { name: /MCP/ });
    fireEvent.click(screen.getByRole("tab", { name: /MCP/ }));

    await waitFor(() => {
      expect(testRouter.state.location.search).toMatchObject({ tab: "mcp" });
    });

    expect(await screen.findByText("本地 MCP 服务状态")).toBeInTheDocument();
    expect(screen.getByText("http://127.0.0.1:53901/mcp")).toBeInTheDocument();
    expect(screen.getByText("指导 AI Agent 安装该 MCP 服务")).toBeInTheDocument();
    expect(screen.getByText("复制安装提示词")).toBeInTheDocument();
  });

  it("switches to 设置迁移 tab and displays SettingsTransfer", async () => {
    const { testRouter } = renderRouterApp("/settings");

    await screen.findByRole("tab", { name: /设置迁移/ });
    fireEvent.click(screen.getByRole("tab", { name: /设置迁移/ }));

    await waitFor(() => {
      expect(testRouter.state.location.search).toMatchObject({ tab: "transfer" });
    });

    expect(await screen.findByText("加密导出")).toBeInTheDocument();
    expect(screen.getByText("导入到此设备")).toBeInTheDocument();
  });

  it("switches to 快捷键 tab and displays ShortcutsTable", async () => {
    const { testRouter } = renderRouterApp("/settings");

    await screen.findByRole("tab", { name: /快捷键/ });
    fireEvent.click(screen.getByRole("tab", { name: /快捷键/ }));

    await waitFor(() => {
      expect(testRouter.state.location.search).toMatchObject({ tab: "shortcuts" });
    });

    expect(await screen.findByText("命令面板快捷指令对照表")).toBeInTheDocument();
    expect(screen.getByText("上课时间表")).toBeInTheDocument();
    expect(screen.getByText("OJ首页")).toBeInTheDocument();
    expect(screen.getByText("最近班级测验")).toBeInTheDocument();
    expect(screen.getByText("UOOC任务tab")).toBeInTheDocument();
    expect(screen.getByText("校园网登录页面")).toBeInTheDocument();
    expect(screen.getByText("校园卡tab")).toBeInTheDocument();
    expect(screen.getByText("查电量tab")).toBeInTheDocument();
    expect(screen.getByText("充电费tab")).toBeInTheDocument();
  });

  it("direct navigation to /settings?focus=ocr opens the 本地模型 tab directly", async () => {
    renderRouterApp("/settings?focus=ocr");

    expect(await screen.findByText("本地离线模型")).toBeInTheDocument();
    expect(screen.getByText("ddddocr")).toBeInTheDocument();
  });

  it("direct navigation to /settings?tab=ai opens the AI配置 tab directly", async () => {
    renderRouterApp("/settings?tab=ai");

    expect(await screen.findByText("AI 连接")).toBeInTheDocument();
  });

  it("opens the about tab directly and renders the app info panel", async () => {
    renderRouterApp("/settings?tab=about");

    // 公共内容快照到达会引起一次重渲染，用 waitFor 反复查询避免拿到被替换的节点。
    await waitFor(() => expect(screen.getByText("泥深 · Your SZU Life")).toBeInTheDocument());
    expect(screen.getByTestId("app-version")).toBeInTheDocument();
  });

  it("shows the diagnostics tab once debug mode is enabled", async () => {
    const original = vi.mocked(invoke).getMockImplementation()!;
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_debug_mode") return { enabled: true, logCount: 0 };
      if (command === "get_log_info")
        return {
          todayLogPath: "/tmp/app_logs/today.log",
          logDir: "/tmp/app_logs",
          totalSizeBytes: 1024,
          fileCount: 0,
          currentLevel: "INFO",
          availableLevels: ["DEBUG", "INFO"],
          files: [],
        };
      return original(command);
    });

    renderRouterApp("/settings?tab=debug");

    expect(await screen.findByRole("tab", { name: /诊断日志/ })).toBeInTheDocument();
    expect(await screen.findByText("日志文件概览")).toBeInTheDocument();
  });
});
