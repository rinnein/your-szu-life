/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { createMemoryHistory, createRouter, RouterProvider } from "@tanstack/react-router";
import { routeTree } from "./routeTree.gen";
import { listen } from "@tauri-apps/api/event";
import { authFailureFor, normalizeAuthState } from "./App";

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
});

function renderRouterApp(initialEntry = "/") {
  const history = createMemoryHistory({ initialEntries: [initialEntry] });
  const testRouter = createRouter({ routeTree, history });
  return { ...render(<RouterProvider router={testRouter} />), testRouter };
}

describe("HeroUI frontend smoke tests", () => {
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

  it("renders the unauthenticated state with a HeroUI button", async () => {
    renderRouterApp();

    expect(await screen.findByRole("heading", { name: "请完成统一认证" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "打开认证窗口" })).toBeInTheDocument();
  });

  it("shows an accessible placeholder alert when a feature is selected", async () => {
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
      return undefined;
    });

    renderRouterApp();
    const feature = await screen.findByRole("button", { name: /课表/ });
    fireEvent.click(feature);

    expect(await screen.findByRole("status")).toHaveTextContent("课表功能即将支持");
    fireEvent.click(screen.getByRole("button", { name: "关闭提示" }));
    await waitFor(() => expect(screen.queryByRole("status")).not.toBeInTheDocument());
  });
  it("opens the accessible account menu and logout confirmation", async () => {
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
      return undefined;
    });

    renderRouterApp();
    fireEvent.click(await screen.findByRole("button", { name: "打开账户菜单" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "注销" }));

    expect(await screen.findByRole("heading", { name: "确认注销？" })).toBeInTheDocument();
  });

  it("renders the workbench HeroUI card with visible slots", async () => {
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
      return undefined;
    });

    renderRouterApp();
    const heroCard = await screen.findByTestId("hero-card");

    expect(heroCard).toHaveAttribute("data-slot", "card");
    expect(heroCard.querySelector('[data-slot="card-content"]')).toBeInTheDocument();
    expect(heroCard).toHaveTextContent("欢迎回来");
    expect(heroCard).toHaveTextContent("从这里开始你的校园生活。");
  });

  it("keeps the account trigger avatar-free and uses semantic feature icons", async () => {
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
      return undefined;
    });

    renderRouterApp();
    const accountTrigger = await screen.findByRole("button", { name: "打开账户菜单" });
    expect(accountTrigger.querySelector('[data-slot="avatar"]')).not.toBeInTheDocument();
    expect(document.querySelectorAll(".feature-icon svg")).toHaveLength(6);
  });

  it("defaults the region tab to staff when the network overview identifies SZU_WLAN", async () => {
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
      if (command === "get_dorm_login_logs") return [];
      if (command === "consume_dorm_login_failure_notice") return null;
      return undefined;
    });

    renderRouterApp("/campus-network");
    expect(await screen.findByText("教工区认证功能暂未支持。")).toBeInTheDocument();
    expect(screen.getByText("当前推测网络状态：未登录(教工区)")).toBeInTheDocument();
  });

  it("renders the campus network zones and dormitory controls", async () => {
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
      if (command === "get_dorm_login_logs") return [];
      if (command === "consume_dorm_login_failure_notice") return null;
      if (command === "save_dorm_network_settings")
        return { enabled: true, credentialMode: "currentAccount", customConfigured: false };
      return undefined;
    });

    const { testRouter } = renderRouterApp("/campus-network");
    expect(await screen.findByRole("heading", { name: "校园网登录" })).toBeInTheDocument();
    expect(document.querySelectorAll(".tabs--secondary")).toHaveLength(0);
    expect(screen.getByRole("tab", { name: "教工区" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "宿舍区" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("tab", { name: "教工区" }));
    expect(screen.getByText("教工区认证功能暂未支持。")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("tab", { name: "宿舍区" }));
    const toggle = screen.getByRole("switch", { name: "开启宿舍区自动登录" });
    fireEvent.click(toggle);
    fireEvent.click(screen.getByRole("button", { name: "保存设置" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_dorm_network_settings", expect.anything()),
    );
    fireEvent.click(screen.getByRole("button", { name: "返回上一级" }));
    await waitFor(() => expect(testRouter.state.location.pathname).toBe("/"));
  });

  it("shows gateway online details when the dorm portal confirms an active session", async () => {
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
      if (command === "get_network_overview")
        return {
          state: "connected",
          region: "dormitory",
          ssid: "SZU_CTC&CMCC",
          portalReachable: true,
          staffHostReachable: false,
          portalSession: {
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
      if (command === "get_dorm_login_logs") return [];
      if (command === "consume_dorm_login_failure_notice") return null;
      return undefined;
    });

    renderRouterApp("/campus-network");
    expect(await screen.findByText("当前推测网络状态：已连接(宿舍区)")).toBeInTheDocument();
    expect(screen.getByText("张三")).toBeInTheDocument();
    expect(screen.getByText("172.24.111.161")).toBeInTheDocument();
    expect(screen.getAllByText("未知/不限")).toHaveLength(2);
  });

  it("renders settings as a real route and navigates back to the workbench", async () => {
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
      return undefined;
    });

    const { testRouter } = renderRouterApp("/settings");
    expect(await screen.findByText("确认账户资料")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "返回工作台" }));
    await waitFor(() => expect(testRouter.state.location.pathname).toBe("/"));
    expect(await screen.findByTestId("hero-card")).toBeInTheDocument();
  });
});
