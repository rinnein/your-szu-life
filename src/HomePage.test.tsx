/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { createMemoryHistory, createRouter, RouterProvider } from "@tanstack/react-router";
import { routeTree } from "./routeTree.gen";
import { listen } from "@tauri-apps/api/event";
import { authFailureFor, normalizeAuthState } from "./auth";
import { registerDebugTap } from "./shell/use-debug-mode";
import { toast } from "./ui";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

describe("authentication recovery UI", () => {
  it("keeps dedicated recovery copy for credential capture failures", () => {
    expect(authFailureFor({ CredentialCaptureFailed: null })).toMatchObject({
      title: "本次登录未能保存凭据",
      action: "重新登录",
    });
  });

  it("distinguishes session persistence and account-mismatch failures", () => {
    expect(authFailureFor({ SessionCaptureFailed: null })?.title).toContain("安全保存会话");
    expect(authFailureFor({ AccountMismatch: null })?.title).toContain("账号不一致");
  });

  it("keeps student-id setup limited to a campus-card supplement", () => {
    expect("补充校园卡号").toContain("校园卡号");
  });
});

describe("auth state transport normalization", () => {
  it("accepts serde's camelCase enum payloads", () => {
    expect(normalizeAuthState("authenticating")).toEqual({ Authenticating: null });
    expect(normalizeAuthState("credentialCaptureFailed")).toEqual({
      CredentialCaptureFailed: null,
    });
    expect(normalizeAuthState("sessionCaptureFailed")).toEqual({
      SessionCaptureFailed: null,
    });
    expect(normalizeAuthState("credentialStorageFailed")).toEqual({
      CredentialStorageFailed: null,
    });
    expect(normalizeAuthState("accountMismatch")).toEqual({ AccountMismatch: null });
    expect(normalizeAuthState({ awaitingProfile: { studentId: "1234567890" } })).toEqual({
      AwaitingProfile: { name: null, studentId: "1234567890" },
    });
  });

  it("recovers student ID from the pre-fix snake_case event payload", () => {
    expect(normalizeAuthState({ awaitingProfile: { student_id: "1234567890" } })).toEqual({
      AwaitingProfile: { name: null, studentId: "1234567890" },
    });
    expect(normalizeAuthState({ Authenticated: { student_id: "1234567890" } })).toEqual({
      Authenticated: { name: null, studentId: "1234567890" },
    });
  });

  it("normalizes terminal event objects from Tauri", () => {
    expect(normalizeAuthState({ sessionExpired: null })).toEqual({ SessionExpired: null });
    expect(normalizeAuthState("webViewUpdateRequired")).toEqual({ WebViewUpdateRequired: null });
    expect(normalizeAuthState({ webViewUpdateRequired: null })).toEqual({
      WebViewUpdateRequired: null,
    });
    expect(authFailureFor({ WebViewUpdateRequired: null })?.detail).toContain("账户资料仍保留");
  });
});

function renderRouterApp(initialEntry = "/") {
  const history = createMemoryHistory({
    initialEntries: initialEntry === "/" ? [initialEntry] : ["/", initialEntry],
  });
  const testRouter = createRouter({ routeTree, history });
  return { ...render(<RouterProvider router={testRouter} />), testRouter };
}

/** 默认按「已认证」应答，`extra` 可以覆盖其余命令。 */
function mockSignedIn(extra?: (command: string) => unknown) {
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
    return extra?.(command);
  });
}

describe("frontend smoke tests", () => {
  afterEach(() => cleanup());

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listen).mockResolvedValue(() => undefined);
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_auth_state") return "unauthenticated";
      if (command === "get_account_profile") return null;
      if (command === "get_user_data") return null;
      return undefined;
    });
  });

  it("renders the unauthenticated state with a primary action", async () => {
    renderRouterApp();

    expect(await screen.findByRole("heading", { name: "请完成统一认证" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "打开认证窗口" })).toBeInTheDocument();
  });

  it("reaches the about panel from the unauthenticated footer link", async () => {
    const { testRouter } = renderRouterApp();
    expect(await screen.findByRole("heading", { name: "请完成统一认证" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /关于 · 检查更新/ }));
    await waitFor(() => expect(testRouter.state.location.pathname).toBe("/settings"));
    expect(await screen.findByRole("heading", { name: "关于" })).toBeInTheDocument();
  });

  it("enters the home page immediately and refreshes the session in the background", async () => {
    mockSignedIn();

    renderRouterApp();
    expect(await screen.findByRole("heading", { name: "首页" })).toBeInTheDocument();
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("refresh_auth_session"));

    const initialCalls = vi
      .mocked(invoke)
      .mock.calls.filter(([command]) => command === "refresh_auth_session").length;
    fireEvent(window, new Event("focus"));
    await waitFor(() =>
      expect(
        vi.mocked(invoke).mock.calls.filter(([command]) => command === "refresh_auth_session")
          .length,
      ).toBeGreaterThan(initialCalls),
    );
  });

  it("does not overwrite automatic recovery with a late initial state response", async () => {
    let resolveState!: (value: unknown) => void;
    const initial = new Promise((resolve) => {
      resolveState = resolve;
    });
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_auth_state") return initial;
      return null;
    });
    renderRouterApp();
    await waitFor(() =>
      expect(listen).toHaveBeenCalledWith("auth-state-changed", expect.any(Function)),
    );
    const handler = vi
      .mocked(listen)
      .mock.calls.find(([name]) => name === "auth-state-changed")![1];
    await act(async () => {
      handler({ event: "auth-state-changed", id: 0, payload: "authenticating" });
    });
    expect(await screen.findByRole("heading", { name: "正在恢复统一认证" })).toBeInTheDocument();
    await act(async () => {
      resolveState({ authenticated: { studentId: "1234567890" } });
    });
    expect(screen.getByRole("heading", { name: "正在恢复统一认证" })).toBeInTheDocument();
  });

  it("requires five taps inside the two-second window", () => {
    expect(registerDebugTap([], 0).activated).toBe(false);
    expect(registerDebugTap([0, 100, 200, 300], 2_100).activated).toBe(false);
    expect(registerDebugTap([0, 100, 200, 300], 1_900).activated).toBe(true);
  });

  it("enables debug mode after five rapid brand-mark taps", async () => {
    let debugEnabled = false;
    mockSignedIn((command) => {
      if (command === "get_debug_mode") return { enabled: debugEnabled, logCount: 0 };
      if (command === "set_debug_mode") {
        debugEnabled = true;
        return { enabled: true, logCount: 0 };
      }
      return undefined;
    });

    renderRouterApp();
    const mark = await screen.findByRole("button", { name: /连续点击 5 次/ });
    for (let index = 0; index < 5; index += 1) fireEvent.click(mark);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_debug_mode", { enabled: true }));
  });

  it("keeps 成绩查询 on the study page with a placeholder toast notification", async () => {
    mockSignedIn();
    const infoSpy = vi.spyOn(toast, "info");

    const { testRouter } = renderRouterApp("/study");
    const feature = await screen.findByRole("button", { name: /^成绩查询/ });
    expect(feature).toHaveTextContent("暂未实现 · 即将支持");
    fireEvent.click(feature);

    expect(infoSpy).toHaveBeenCalledWith("成绩查询功能即将支持", {
      description: "当前仅提供入口占位",
    });
    expect(testRouter.state.location.pathname).toBe("/study");
    infoSpy.mockRestore();
  });

  it("opens the accessible account menu and logout confirmation", async () => {
    mockSignedIn();

    renderRouterApp();
    fireEvent.click(await screen.findByRole("button", { name: "打开账户菜单" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "注销" }));

    expect(await screen.findByRole("heading", { name: "确认注销？" })).toBeInTheDocument();
  });

  it("anchors the account menu to the account trigger", async () => {
    mockSignedIn();

    renderRouterApp();
    await screen.findByRole("button", { name: "打开账户菜单" });

    const trigger = document.querySelector("m-button.account-trigger");
    const menu = document.querySelector("m-menu") as
      | (HTMLElement & { anchor?: Element | null })
      | null;

    expect(trigger).not.toBeNull();
    expect(menu).not.toBeNull();
    expect(menu!.anchor).toBe(trigger);
  });

  it("toggles the account menu from the trigger on repeated clicks", async () => {
    mockSignedIn();

    renderRouterApp();
    const trigger = await screen.findByRole("button", { name: "打开账户菜单" });
    const menu = document.querySelector("m-menu");

    fireEvent.click(trigger);
    await waitFor(() => expect(menu).toHaveAttribute("open"));

    fireEvent.click(trigger);
    await waitFor(() => expect(menu).not.toHaveAttribute("open"));
  });

  it("renders the home announcement board", async () => {
    mockSignedIn();

    renderRouterApp();
    const heroCard = await screen.findByTestId("hero-card");

    expect(heroCard).toHaveAttribute("data-slot", "card");
    expect(heroCard.querySelector('[data-slot="card-content"]')).toBeInTheDocument();
    expect(heroCard).toHaveTextContent("欢迎回来");
    expect(heroCard).toHaveTextContent("从这里开始你的校园生活。");
  });

  it("renders the home calendar and schedule widget", async () => {
    mockSignedIn();

    renderRouterApp();
    expect(await screen.findByRole("heading", { name: "日历与待办" })).toBeInTheDocument();
  });

  it("keeps the account trigger avatar-free and the top bar as the only header", async () => {
    mockSignedIn();

    renderRouterApp();
    const accountTrigger = await screen.findByRole("button", { name: "打开账户菜单" });
    expect(accountTrigger.querySelector('[data-slot="avatar"]')).not.toBeInTheDocument();
    // 首页是逻辑栈 root：顶栏不提供返回按钮，只提供命令面板搜索入口。
    expect(screen.queryByRole("button", { name: "返回上一页" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "打开命令面板搜索" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /打开命令面板快捷搜索/ })).not.toBeInTheDocument();
  });

  it("lists study entries with semantic feature icons", async () => {
    mockSignedIn();

    renderRouterApp("/study");
    expect(await screen.findByRole("heading", { name: "学习" })).toBeInTheDocument();
    expect(document.querySelectorAll(".feature-icon svg")).toHaveLength(7);
    for (const [label, description] of [
      ["上课时间表", "查看各节次上课时间"],
      ["OJ", "班级、测验与编程练习"],
      ["UOOC刷课", "优课在线视频任务"],
      ["畅课 lms", "课程待办与站内通知"],
      ["选课", "查课、抢课与选课期间课表"],
      ["学业完成查询", "培养方案、学分与课程完成情况"],
      ["成绩查询", "暂未实现 · 即将支持"],
    ]) {
      expect(screen.getByRole("button", { name: new RegExp(`^${label}`) })).toHaveTextContent(
        description,
      );
    }
  });

  it("lists life entries with semantic feature icons", async () => {
    mockSignedIn();

    renderRouterApp("/life");
    expect(await screen.findByRole("heading", { name: "生活" })).toBeInTheDocument();
    expect(document.querySelectorAll(".feature-icon svg")).toHaveLength(2);
    for (const [label, description] of [
      ["校园网登录", "教工区 + 宿舍区 | 自动登录"],
      ["校园卡/电费/网费", "快速导航入口"],
    ]) {
      expect(screen.getByRole("button", { name: new RegExp(`^${label}`) })).toHaveTextContent(
        description,
      );
    }
  });

  it("defaults the region tab to staff when the network overview identifies SZU_WLAN", async () => {
    mockSignedIn((command) => {
      if (command === "get_network_overview")
        return {
          state: "notLoggedIn",
          region: "staff",
          ssid: "SZU_WLAN",
          portalReachable: false,
          staffHostReachable: true,
        };
      if (command === "get_dorm_network_settings")
        return { enabled: false, credentialMode: "currentAccount", customConfigured: false };
      if (command === "get_dorm_login_status")
        return {
          state: "disabled",
          connected: false,
          portalReachable: false,
          consecutiveFailures: 0,
        };
      if (command === "consume_dorm_login_failure_notice") return null;
      return undefined;
    });

    renderRouterApp("/life/campus-network");
    expect(await screen.findByText("教工区登录")).toBeInTheDocument();
    expect(screen.getByText("当前推测网络状态：未登录(教工区)")).toBeInTheDocument();
  });

  it("navigates from the life page to the campus network controls and back", async () => {
    mockSignedIn((command) => {
      if (command === "get_network_overview")
        return {
          state: "noNetwork",
          region: "none",
          portalReachable: false,
          staffHostReachable: false,
        };
      if (command === "get_dorm_network_settings")
        return { enabled: false, credentialMode: "currentAccount", customConfigured: false };
      if (command === "get_dorm_login_status")
        return {
          state: "disabled",
          connected: false,
          portalReachable: false,
          consecutiveFailures: 0,
        };
      if (command === "consume_dorm_login_failure_notice") return null;
      if (command === "save_dorm_network_settings")
        return { enabled: true, credentialMode: "currentAccount", customConfigured: false };
      return undefined;
    });

    const { testRouter } = renderRouterApp("/life");
    fireEvent.click(await screen.findByRole("button", { name: /校园网登录/ }));
    await waitFor(() => expect(testRouter.state.location.pathname).toBe("/life/campus-network"));
    expect(await screen.findByRole("heading", { name: "校园网登录" })).toBeInTheDocument();
    expect(document.querySelectorAll(".tabs--secondary")).toHaveLength(0);
    expect(screen.getByRole("tab", { name: "教工区" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "宿舍区" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("tab", { name: "教工区" }));
    expect(screen.getByText("教工区登录")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("tab", { name: "宿舍区" }));
    const toggle = screen.getByRole("switch", { name: "开启宿舍区自动登录" });
    fireEvent.click(toggle);
    fireEvent.click(screen.getByRole("button", { name: "保存设置" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_dorm_network_settings", expect.anything()),
    );
    // 嵌套后上一级是「生活」页，不再是首页。
    fireEvent.click(screen.getByRole("button", { name: "返回上一页" }));
    await waitFor(() => expect(testRouter.state.location.pathname).toBe("/life"));
  });

  it("shows gateway online details when the dorm portal confirms an active session", async () => {
    mockSignedIn((command) => {
      if (command === "get_network_overview")
        return {
          state: "connected",
          region: "dormitory",
          ssid: "SZU_CTC&CMCC",
          portalReachable: true,
          staffHostReachable: false,
          portalSession: {
            campusCardNumber: "654321",
            onlineUsername: "张三",
            onlineDuration: 4294967295,
            onlineFlow: 4294967295,
            loginIp: "172.24.111.161",
            loginStartedAt: "2026-09-05 09:06:33",
            loginExpiresAt: "2026-09-07 19:30:49",
            charset: "gb2312",
          },
        };
      if (command === "get_dorm_network_settings")
        return { enabled: false, credentialMode: "currentAccount", customConfigured: false };
      if (command === "get_dorm_login_status")
        return { state: "online", connected: true, portalReachable: true, consecutiveFailures: 0 };
      if (command === "consume_dorm_login_failure_notice") return null;
      return undefined;
    });

    renderRouterApp("/life/campus-network");
    expect(await screen.findByText("当前推测网络状态：已连接(宿舍区)")).toBeInTheDocument();
    expect(screen.getByText("张三")).toBeInTheDocument();
    expect(screen.getByText("654321")).toBeInTheDocument();
    expect(screen.getByText("172.24.111.161")).toBeInTheDocument();
    expect(screen.getAllByText("未知/不限")).toHaveLength(2);
  });

  it("renders settings as a top-level route without its own back button", async () => {
    mockSignedIn();

    renderRouterApp("/settings");
    expect(await screen.findByText("确认账户资料")).toBeInTheDocument();
    // 设置是顶级部件之一，靠导航部件回首页，顶栏不再给返回键。
    expect(screen.queryByRole("button", { name: "返回上一页" })).not.toBeInTheDocument();
  });
});
