import { LogicalNavigationProvider } from "./logical-navigation";
/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  RouterProvider,
} from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import CampusNetworkPage from "./CampusNetworkPage";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

type Overview = {
  state: "connected" | "notLoggedIn" | "noNetwork" | "unknown";
  connected?: boolean;
  region: "dormitory" | "staff" | "none";
  portalReachable: boolean;
  staffHostReachable: boolean;
  staffSession?: {
    username: string;
    realName?: string | null;
    loginIp: string;
    loginIpv6?: string | null;
    userMac?: string | null;
    usedBytes?: number | null;
    usedSeconds?: number | null;
    loginStartedAtMs?: number | null;
  } | null;
  portalSession?: {
    campusCardNumber?: string | null;
    onlineUsername?: string | null;
  } | null;
};

let overview: Overview;
let settings: {
  enabled: boolean;
  credentialMode: "currentAccount" | "custom";
  customConfigured: boolean;
  customCampusCardNumber?: string | null;
};
let staffSettings: {
  revision: number;
  mode: "macAuth" | "appAuto";
  enabled: boolean;
  credentialMode: "currentAccount" | "custom";
  customConfigured: boolean;
  customCampusCardNumber?: string | null;
  needsApply: boolean;
  appliedMode?: "macAuth" | "appAuto" | null;
};
let logoutHandler: () => Promise<unknown>;
let permissionHandler: () => Promise<unknown>;
let refreshHandler: () => Promise<unknown>;
const listeners = new Map<string, EventCallback<unknown>>();
const cardLabel = "校园卡号";
const nameLabel = "用户姓名";
const valueFor = (label: string) => screen.getByText(label).nextElementSibling;

async function renderPage(readyLabel = cardLabel) {
  const history = createMemoryHistory({ initialEntries: ["/"] });
  const router = createRouter({
    routeTree: createRootRoute({
      component: () => (
        <LogicalNavigationProvider>
          <CampusNetworkPage />
        </LogicalNavigationProvider>
      ),
    }),
    history,
  });
  // Route integration is covered in App.test.tsx; these are component state tests.
  await router.load();
  render(<RouterProvider router={router} />);
  await screen.findByText(readyLabel);
}

function publish(next: Overview) {
  act(() => {
    listeners.get("network-overview-changed")?.({
      event: "network-overview-changed",
      id: 1,
      payload: next,
    });
  });
}

function publishSettings() {
  act(() => {
    listeners.get("dorm-network-settings-changed")?.({
      event: "dorm-network-settings-changed",
      id: 1,
      payload: { ...settings },
    });
  });
}

async function openDisconnectDialog() {
  fireEvent.click(screen.getByRole("button", { name: "断开连接" }));
  return screen.findByRole("alertdialog");
}

const logoutCalls = () =>
  vi.mocked(invoke).mock.calls.filter(([name]) => name === "logout_campus_network");

describe("campus network status and logout", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listeners.clear();
    settings = { enabled: false, credentialMode: "currentAccount", customConfigured: false };
    staffSettings = {
      revision: 0,
      mode: "appAuto",
      enabled: false,
      credentialMode: "currentAccount",
      customConfigured: false,
      customCampusCardNumber: null,
      needsApply: true,
      appliedMode: null,
    };
    permissionHandler = async () => undefined;
    refreshHandler = async () => overview;
    logoutHandler = async () => {
      settings = { ...settings, enabled: false };
      overview = { ...overview, state: "notLoggedIn", portalSession: null, staffSession: null };
      publishSettings();
      return {
        outcome: "disconnected",
        region: overview.region,
        message: "已确认当前设备断开校园网；自动连接已关闭。",
      };
    };
    overview = {
      state: "connected",
      region: "dormitory",
      portalReachable: true,
      staffHostReachable: false,
      portalSession: { campusCardNumber: "654321", onlineUsername: "网关同学" },
    };
    vi.mocked(listen).mockImplementation(async (event, callback) => {
      listeners.set(event, callback as EventCallback<unknown>);
      return () => {
        listeners.delete(event);
      };
    });
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_auth_state")
        return { authenticated: { studentId: "1234567890", name: "应用同学" } };
      if (command === "get_account_profile")
        return {
          studentId: "1234567890",
          campusCardNumber: "123456",
          name: "应用同学",
          hasPassword: true,
        };
      if (command === "get_user_data") return { studentId: "1234567890", name: "应用同学" };
      if (command === "request_campus_network_permissions") return permissionHandler();
      if (command === "get_network_overview") return overview;
      if (command === "refresh_campus_network") return refreshHandler();
      if (command === "get_dorm_network_settings") return { ...settings };
      if (command === "get_staff_network_settings") return { ...staffSettings };
      if (command === "logout_campus_network") return logoutHandler();
      if (command === "get_dorm_login_status")
        return {
          state: "disabled",
          connected: true,
          portalReachable: true,
          consecutiveFailures: 0,
        };
      if (command === "consume_dorm_login_failure_notice") return null;
      return undefined;
    });
  });
  afterEach(cleanup);

  it("requests Android network permissions without blocking the first refresh", async () => {
    permissionHandler = async () => {
      throw new Error("permission denied");
    };
    await renderPage();
    expect(
      vi.mocked(invoke).mock.calls.some(([name]) => name === "request_campus_network_permissions"),
    ).toBe(true);
    expect(valueFor(cardLabel)).toHaveTextContent("654321");
  });

  it("does not let a cached refresh overwrite a newer overview event", async () => {
    let resolveRefresh!: (value: Overview) => void;
    refreshHandler = () =>
      new Promise<Overview>((resolve) => {
        resolveRefresh = resolve;
      });
    await renderPage("校园网登录");
    const latest: Overview = {
      state: "unknown",
      connected: true,
      region: "none",
      portalReachable: true,
      staffHostReachable: true,
      portalSession: null,
      staffSession: null,
    };
    publish(latest);
    resolveRefresh(overview);
    await waitFor(() => expect(valueFor("网络连接")).toHaveTextContent("已连接网络"));
    expect(screen.getByText("当前推测网络状态：已连接(未识别区域)")).toBeInTheDocument();
  });

  it("refreshes connection status from the status card", async () => {
    overview = {
      state: "noNetwork",
      connected: false,
      region: "none",
      portalReachable: false,
      staffHostReachable: false,
      portalSession: null,
      staffSession: null,
    };
    await renderPage("校园网登录");
    overview = {
      ...overview,
      state: "unknown",
      connected: true,
      region: "none",
      portalReachable: true,
      staffHostReachable: false,
    };
    fireEvent.click(screen.getByRole("button", { name: "刷新连接状态" }));
    await waitFor(() => expect(valueFor("网络连接")).toHaveTextContent("已连接网络"));
    expect(vi.mocked(invoke).mock.calls.some(([name]) => name === "refresh_campus_network")).toBe(
      true,
    );
  });

  it("shows a connected but unclassified network when both portals are unavailable", async () => {
    overview = {
      state: "unknown",
      connected: true,
      region: "none",
      portalReachable: false,
      staffHostReachable: false,
      portalSession: null,
      staffSession: null,
    };
    await renderPage("校园网登录");
    expect(screen.getByText("当前推测网络状态：已连接(未识别区域)")).toBeInTheDocument();
    expect(valueFor("网络连接")).toHaveTextContent("已连接网络");
    expect(valueFor("宿舍区门户")).toHaveTextContent("不可达");
    expect(valueFor("教工区门户")).toHaveTextContent("不可达");
  });

  it("shows the gateway uid and Chinese NID instead of the application profile", async () => {
    await renderPage();
    expect(valueFor(cardLabel)).toHaveTextContent("654321");
    expect(valueFor(nameLabel)).toHaveTextContent("网关同学");
    expect(valueFor(cardLabel)).not.toHaveTextContent("123456");
    expect(valueFor(nameLabel)).not.toHaveTextContent("应用同学");
  });

  it("prefills the saved custom dormitory card number", async () => {
    settings = {
      enabled: false,
      credentialMode: "custom",
      customConfigured: true,
      customCampusCardNumber: "001234",
    };
    await renderPage("校园网登录");
    fireEvent.click(screen.getByRole("tab", { name: "单独配置" }));
    expect(screen.getByLabelText("校园卡号")).toHaveValue("001234");
    expect(screen.getByLabelText("密码")).toHaveValue("");
  });

  it("prefills the saved custom staff card number", async () => {
    staffSettings = {
      ...staffSettings,
      credentialMode: "custom",
      customConfigured: true,
      customCampusCardNumber: "654321",
    };
    await renderPage("校园网登录");
    fireEvent.click(screen.getByRole("tab", { name: "教工区" }));
    fireEvent.click(screen.getByRole("tab", { name: "软件自动登录（不绑定MAC地址）" }));
    fireEvent.click(screen.getByRole("tab", { name: "单独配置" }));
    expect(screen.getByLabelText("校园卡号")).toHaveValue("654321");
    expect(screen.getByLabelText("密码")).toHaveValue("");
  });

  it.each(["campusCardNumber", "onlineUsername"] as const)(
    "handles a missing %s independently",
    async (field) => {
      overview.portalSession = { ...overview.portalSession, [field]: null };
      await renderPage();
      expect(valueFor(field === "campusCardNumber" ? cardLabel : nameLabel)).toHaveTextContent(
        "网关未提供",
      );
      expect(valueFor(field === "campusCardNumber" ? nameLabel : cardLabel)).toHaveTextContent(
        field === "campusCardNumber" ? "网关同学" : "654321",
      );
    },
  );

  it("handles an absent session without falling back to application credentials", async () => {
    overview.portalSession = null;
    await renderPage();
    expect(valueFor(cardLabel)).toHaveTextContent("网关未提供");
    expect(valueFor(nameLabel)).toHaveTextContent("网关未提供");
  });

  it("refreshes both values from overview events and preserves leading zeroes", async () => {
    await renderPage();
    publish({
      ...overview,
      portalSession: { campusCardNumber: "001234", onlineUsername: "新同学" },
    });
    expect(valueFor(cardLabel)).toHaveTextContent("001234");
    expect(valueFor(nameLabel)).toHaveTextContent("新同学");
    expect(screen.queryByText("654321")).not.toBeInTheDocument();
    expect(screen.queryByText("网关同学")).not.toBeInTheDocument();
  });

  it.each([
    ["notLoggedIn", "dormitory"],
    ["noNetwork", "none"],
    ["connected", "staff"],
  ] as const)("hides stale identity for %s / %s", async (state, region) => {
    await renderPage();
    publish({ ...overview, state, region });
    expect(screen.queryByText(cardLabel)).not.toBeInTheDocument();
    expect(screen.queryByText(nameLabel)).not.toBeInTheDocument();
    expect(screen.queryByText("654321")).not.toBeInTheDocument();
    expect(screen.queryByText("网关同学")).not.toBeInTheDocument();
  });

  it("renders gateway strings as text, never as HTML", async () => {
    overview.portalSession = {
      campusCardNumber: "001234",
      onlineUsername: '<img src=x onerror="alert(1)">',
    };
    await renderPage();
    expect(valueFor(nameLabel)).toHaveTextContent('<img src=x onerror="alert(1)">');
    expect(valueFor(nameLabel)?.querySelector("img")).toBeNull();
  });

  it("shows staff online information and cumulative units instead of dormitory identity", async () => {
    overview = {
      ...overview,
      region: "staff",
      staffHostReachable: true,
      staffSession: {
        username: "001234",
        realName: "教工区同学",
        loginIp: "172.28.0.8",
        loginIpv6: "fd7a::1234",
        userMac: "0000-0000-0001",
        usedBytes: 1024 ** 3,
        usedSeconds: 3661,
        loginStartedAtMs: 1788827879000,
      },
    };
    await renderPage("教工区在线详情");
    expect(screen.getByText("当前推测网络状态：已连接(教工区)")).toBeInTheDocument();
    expect(valueFor("登录账号")).toHaveTextContent("001234");
    expect(valueFor("姓名")).toHaveTextContent("教工区同学");
    expect(valueFor("MAC 地址")).toHaveTextContent("0000-0000-0001");
    expect(valueFor("累计已用流量")).toHaveTextContent("1 GiB");
    expect(valueFor("累计已用时长")).toHaveTextContent("1 小时 1 分 1 秒");
    expect(screen.queryByText("网关同学")).not.toBeInTheDocument();
    expect(screen.queryByText("654321")).not.toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "教工区" })).toHaveAttribute("aria-selected", "true");
  });

  it("distinguishes zero usage from absent staff fields and hides stale data when unknown", async () => {
    overview = {
      ...overview,
      region: "staff",
      staffSession: { username: "001234", loginIp: "172.28.0.8", usedBytes: 0 },
    };
    await renderPage("教工区在线详情");
    expect(valueFor("累计已用流量")).toHaveTextContent("0 B");
    expect(valueFor("累计已用时长")).toHaveTextContent("网关未提供");
    expect(valueFor("姓名")).toHaveTextContent("网关未提供");
    publish({ ...overview, state: "unknown" });
    expect(screen.getByText("当前推测网络状态：状态待确认(教工区)")).toBeInTheDocument();
    expect(screen.queryByText("001234")).not.toBeInTheDocument();
    expect(screen.queryByText("教工区在线详情")).not.toBeInTheDocument();
  });

  it("renders untrusted staff values as text", async () => {
    overview = {
      ...overview,
      region: "staff",
      staffSession: {
        username: "001234",
        realName: '<img src=x onerror="alert(1)">',
        loginIp: "172.28.0.8",
      },
    };
    await renderPage("教工区在线详情");
    expect(valueFor("姓名")).toHaveTextContent('<img src=x onerror="alert(1)">');
    expect(valueFor("姓名")?.querySelector("img")).toBeNull();
  });

  it("does not disconnect or change settings when confirmation is cancelled", async () => {
    settings.enabled = true;
    await renderPage();
    const dialog = await openDisconnectDialog();
    expect(within(dialog).getByText(/不会退出应用账号/)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "取消" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(logoutCalls()).toHaveLength(0);
    expect(screen.getByRole("switch", { name: "开启宿舍区自动登录" })).toBeChecked();
    expect(
      vi.mocked(invoke).mock.calls.some(([name]) => name === "save_dorm_network_settings"),
    ).toBe(false);
  });

  it("invokes only the dedicated logout command and immediately synchronizes the disabled setting", async () => {
    settings.enabled = true;
    await renderPage();
    const dialog = await openDisconnectDialog();
    fireEvent.click(within(dialog).getByRole("button", { name: "确认断开" }));
    await screen.findByText("已断开校园网");
    expect(logoutCalls()).toEqual([["logout_campus_network"]]);
    expect(screen.getByRole("switch", { name: "开启宿舍区自动登录" })).not.toBeChecked();
    expect(screen.getByText("当前推测网络状态：未登录(宿舍区)")).toBeInTheDocument();
    expect(
      vi
        .mocked(invoke)
        .mock.calls.some(
          ([name]) =>
            name === "logout" ||
            name === "save_dorm_network_settings" ||
            name === "trigger_dorm_login",
        ),
    ).toBe(false);
  });

  it("disables repeated actions while a logout is in flight and consumes settings events before completion", async () => {
    settings.enabled = true;
    let complete: (value: unknown) => void = () => {};
    logoutHandler = () =>
      new Promise((resolve) => {
        complete = resolve;
      });
    await renderPage();
    const dialog = await openDisconnectDialog();
    const confirm = within(dialog).getByRole("button", { name: "确认断开" });
    fireEvent.click(confirm);
    await waitFor(() => expect(logoutCalls()).toHaveLength(1));
    expect(confirm).toBeDisabled();
    settings.enabled = false;
    publishSettings();
    expect(screen.getByLabelText("开启宿舍区自动登录")).not.toBeChecked();
    expect(screen.getByLabelText("开启宿舍区自动登录")).toBeDisabled();
    fireEvent.click(confirm);
    expect(logoutCalls()).toHaveLength(1);
    await act(async () => {
      overview = { ...overview, state: "notLoggedIn", portalSession: null };
      complete({
        outcome: "disconnected",
        region: "dormitory",
        message: "已断开；自动连接已关闭。",
      });
    });
    await screen.findByText("已断开校园网");
    expect(screen.getByRole("switch", { name: "开启宿舍区自动登录" })).not.toBeChecked();
  });

  it.each([
    ["failed", "断开未完成"],
    ["unknown", "断开结果待确认"],
  ] as const)(
    "keeps auto-connect disabled and reports %s without claiming success",
    async (outcome, title) => {
      settings.enabled = true;
      logoutHandler = async () => {
        settings.enabled = false;
        publishSettings();
        return { outcome, region: "dormitory", message: "请稍后检查；自动连接已关闭。" };
      };
      await renderPage();
      const dialog = await openDisconnectDialog();
      fireEvent.click(within(dialog).getByRole("button", { name: "确认断开" }));
      await screen.findByText(title);
      expect(screen.queryByText("已断开校园网")).not.toBeInTheDocument();
      expect(screen.getByRole("switch", { name: "开启宿舍区自动登录" })).not.toBeChecked();
      expect(logoutCalls()).toHaveLength(1);
    },
  );

  it("shows command failures and does not restore an enabled draft", async () => {
    settings.enabled = true;
    logoutHandler = async () => {
      settings.enabled = false;
      publishSettings();
      throw new Error("保存关闭设置失败，未发送注销请求");
    };
    await renderPage();
    const dialog = await openDisconnectDialog();
    fireEvent.click(within(dialog).getByRole("button", { name: "确认断开" }));
    await screen.findByText(/保存关闭设置失败，未发送注销请求/);
    expect(screen.getByRole("switch", { name: "开启宿舍区自动登录" })).not.toBeChecked();
    expect(screen.queryByText("已断开校园网")).not.toBeInTheDocument();
  });

  it("does not use the selected tab to choose a logout target", async () => {
    overview = {
      ...overview,
      region: "staff",
      staffSession: { username: "001234", loginIp: "172.28.0.8" },
    };
    await renderPage("教工区在线详情");
    fireEvent.click(screen.getByRole("tab", { name: "宿舍区" }));
    const dialog = await openDisconnectDialog();
    fireEvent.click(within(dialog).getByRole("button", { name: "确认断开" }));
    await screen.findByText("已断开校园网");
    expect(logoutCalls()).toEqual([["logout_campus_network"]]);
    expect(screen.getByText("当前推测网络状态：未登录(教工区)")).toBeInTheDocument();
  });

  it("does not let a late initial settings response overwrite a newer disabled event", async () => {
    const implementation = vi.mocked(invoke).getMockImplementation()!;
    let resolveSettings: (value: unknown) => void = () => {};
    const pendingSettings = new Promise((resolve) => {
      resolveSettings = resolve;
    });
    vi.mocked(invoke).mockImplementation(async (command, args, options) =>
      command === "get_dorm_network_settings"
        ? pendingSettings
        : implementation(command, args, options),
    );
    await renderPage("校园网登录");
    publishSettings();
    await act(async () => {
      resolveSettings({ enabled: true, credentialMode: "currentAccount", customConfigured: false });
    });
    await screen.findByText(cardLabel);
    expect(screen.getByRole("switch", { name: "开启宿舍区自动登录" })).not.toBeChecked();
  });

  it("disposes subscriptions that finish registering after unmount", async () => {
    const pending: (() => void)[] = [];
    const disposers: ReturnType<typeof vi.fn>[] = [];
    vi.mocked(listen).mockImplementation(
      () =>
        new Promise((resolve) => {
          const dispose = vi.fn();
          disposers.push(dispose);
          pending.push(() => resolve(dispose));
        }),
    );
    await renderPage();
    cleanup();
    await act(async () => {
      for (const resolve of pending) resolve();
    });
    expect(disposers).toHaveLength(5);
    for (const dispose of disposers) expect(dispose).toHaveBeenCalledOnce();
  });

  it("waits for a meaningful initial region and then preserves the user's tab choice", async () => {
    overview = { ...overview, state: "noNetwork", region: "none", portalSession: null };
    await renderPage("校园网登录");
    const staff: Overview = {
      ...overview,
      state: "connected",
      region: "staff",
      staffSession: { username: "001234", loginIp: "172.28.0.8" },
    };
    publish(staff);
    expect(screen.getByRole("tab", { name: "教工区" })).toHaveAttribute("aria-selected", "true");
    fireEvent.click(screen.getByRole("tab", { name: "宿舍区" }));
    publish(staff);
    expect(screen.getByRole("tab", { name: "宿舍区" })).toHaveAttribute("aria-selected", "true");
  });

  it("does not let an earlier save response undo a newer disabled settings event", async () => {
    const implementation = vi.mocked(invoke).getMockImplementation()!;
    let completeSave: (value: unknown) => void = () => {};
    const pendingSave = new Promise((resolve) => {
      completeSave = resolve;
    });
    vi.mocked(invoke).mockImplementation(async (command, args, options) =>
      command === "save_dorm_network_settings"
        ? pendingSave
        : implementation(command, args, options),
    );
    await renderPage();
    fireEvent.click(screen.getByRole("switch", { name: "开启宿舍区自动登录" }));
    fireEvent.click(screen.getByRole("button", { name: "保存设置" }));
    await waitFor(() =>
      expect(
        vi.mocked(invoke).mock.calls.some(([name]) => name === "save_dorm_network_settings"),
      ).toBe(true),
    );
    publishSettings();
    await act(async () => {
      completeSave({ enabled: true, credentialMode: "currentAccount", customConfigured: false });
    });
    expect(screen.getByRole("switch", { name: "开启宿舍区自动登录" })).not.toBeChecked();
  });
});
