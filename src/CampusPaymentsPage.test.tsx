/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { StrictMode, type ComponentProps } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { createMemoryHistory, createRouter, RouterProvider } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { routeTree } from "./routeTree.gen";
import { WECHAT_SERVICES, type NetworkFeePortalNotice } from "./campus-payments";
import { hostOf } from "./ui/testing";

const qrProps = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("qrcode.react", async (importOriginal) => {
  const actual = await importOriginal<typeof import("qrcode.react")>();
  return {
    ...actual,
    QRCodeSVG: (props: ComponentProps<typeof actual.QRCodeSVG>) => {
      qrProps(props);
      return <actual.QRCodeSVG {...props} />;
    },
  };
});

const profile = {
  studentId: "1234567890",
  campusCardNumber: "001234",
  hasPassword: true,
  name: "测试同学",
};
let authState: unknown;
const listeners = new Map<string, (event: { payload: unknown }) => void>();
const originalClipboard = Object.getOwnPropertyDescriptor(navigator, "clipboard");
const originalExecCommand = Object.getOwnPropertyDescriptor(document, "execCommand");

function defaultCommand(command: string) {
  if (command === "get_auth_state") return authState;
  if (command === "get_account_profile" || command === "get_user_data") return profile;
  if (command === "get_debug_mode") return { enabled: false, logCount: 0 };
  if (command === "get_electricity_options") return [];
  if (command === "get_electricity_settings") return null;
  return null;
}

function renderAt(path = "/campus-payments", strict = false) {
  const router = createRouter({
    routeTree,
    history: createMemoryHistory({ initialEntries: path === "/" ? [path] : ["/", path] }),
  });
  const content = <RouterProvider router={router} />;
  render(strict ? <StrictMode>{content}</StrictMode> : content);
  return router;
}

function clipboard(value: unknown) {
  Object.defineProperty(navigator, "clipboard", { configurable: true, value });
}

function fallback(value: unknown) {
  Object.defineProperty(document, "execCommand", { configurable: true, value });
}

describe("campus payments route", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.spyOn(window, "scrollTo").mockImplementation(() => {});
    authState = { authenticated: { studentId: profile.studentId, name: profile.name } };
    listeners.clear();
    vi.mocked(invoke).mockImplementation(async (command) => defaultCommand(command));
    vi.mocked(listen).mockImplementation(async (event, callback) => {
      listeners.set(event, callback as (event: { payload: unknown }) => void);
      return () => listeners.delete(event);
    });
    clipboard({ writeText: vi.fn().mockResolvedValue(undefined) });
    fallback(undefined);
  });
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    if (originalClipboard) Object.defineProperty(navigator, "clipboard", originalClipboard);
    else Reflect.deleteProperty(navigator, "clipboard");
    if (originalExecCommand) Object.defineProperty(document, "execCommand", originalExecCommand);
    else Reflect.deleteProperty(document, "execCommand");
  });

  it("replaces the workbench placeholder with the generated route and returns without changing authentication", async () => {
    const router = renderAt("/");
    const entry = await screen.findByRole("button", { name: /校园卡\/电费\/网费/ });
    expect(entry).not.toHaveTextContent("暂未实现");
    expect(screen.queryByRole("button", { name: /网费与电费/ })).not.toBeInTheDocument();
    fireEvent.click(entry);
    expect(await screen.findByRole("heading", { name: "校园卡/电费/网费" })).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/campus-payments/electricity");
    const back = screen.getByRole("button", { name: "返回工作台" });
    // M3 顶栏的返回按钮是原生 button；data-webview-navigation 让 Android
    // 原生工具栏挂载时隐藏它，两处必须同时成立。
    expect(back).toHaveAttribute("data-webview-navigation");
    expect(back.querySelector("svg")).toBeInTheDocument();
    fireEvent.click(back);
    await waitFor(() => expect(router.state.location.pathname).toBe("/"));
    // 工作台改用 MD3 卡片式入口。
    expect(await screen.findByRole("heading", { name: "工作台" })).toBeInTheDocument();
    for (const command of ["begin_authentication", "logout", "capture_auth_credentials"]) {
      expect(invoke).not.toHaveBeenCalledWith(command);
    }
  });

  it.each([
    "unauthenticated",
    "authenticating",
    "sessionExpired",
    { awaitingProfile: { studentId: profile.studentId } },
  ])("returns unfinished authentication to the existing auth flow: %j", async (state) => {
    authState = state;
    const router = renderAt();
    await waitFor(() => expect(router.state.location.pathname).toBe("/"));
    expect(screen.queryByRole("heading", { name: "校园卡/电费/网费" })).not.toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("open_network_fee_portal");
    expect(invoke).not.toHaveBeenCalledWith("consume_network_fee_portal_notice");
    expect(qrProps).not.toHaveBeenCalled();
  });

  it("waits for authentication checking instead of exposing services or redirecting early", async () => {
    authState = "checking";
    const router = renderAt();
    // 加载指示器的宿主与内部元素都带同一个可访问名称，取首个即可。
    const [spinner] = await screen.findAllByLabelText("正在检查统一认证状态");
    expect(spinner).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/campus-payments");
    expect(qrProps).not.toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalledWith("consume_network_fee_portal_notice");
  });

  it("leaves the route when the app session expires", async () => {
    const router = renderAt();
    await screen.findByRole("heading", { name: "校园卡/电费/网费" });
    act(() => listeners.get("auth-state-changed")?.({ payload: "sessionExpired" }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/"));
    expect(await screen.findByRole("heading", { name: "统一认证已过期" })).toBeInTheDocument();
  });

  it("keeps network instructions on the network subroute", async () => {
    renderAt("/campus-payments/network");
    const open = await screen.findByRole("button", { name: "打开网费官网" });
    const guidance = screen.getByRole("region", { name: "网费办理说明" });
    expect(guidance).toHaveTextContent("请先在校园卡「通用代扣费账户」充值足够金额");
    expect(guidance).toHaveTextContent(
      "若当前套餐仍剩余多个月，请在套餐剩余一个月内预订下个月套餐；只有无套餐或套餐剩余一个月内，才能修改或续费套餐",
    );
    expect(guidance.compareDocumentPosition(open) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.queryByText("复制链接后在微信内打开，或使用微信扫码")).not.toBeInTheDocument();
  });

  const routeForService = (service: (typeof WECHAT_SERVICES)[number]) =>
    service.id === "electricity" ? "/campus-payments/recharge" : "/campus-payments/card";

  it.each(WECHAT_SERVICES)(
    "locally renders the %s QR SVG with the entire official URL",
    async (service) => {
      renderAt(routeForService(service));
      await screen.findByRole("heading", { name: "校园卡/电费/网费" });
      const qr = await screen.findByRole("img", { name: service.title + "微信二维码" });
      expect(qr.tagName.toLowerCase()).toBe("svg");
      expect(qr.querySelector("path")).toBeInTheDocument();
      expect(qrProps).toHaveBeenCalledWith(
        expect.objectContaining({ value: service.url, marginSize: 4 }),
      );
      const link = screen.getByLabelText(service.title + "完整链接");
      expect(link).toHaveTextContent(service.url);
      expect(link).toHaveAttribute("tabindex", "0");
      expect(document.querySelector(".payments-layout img[src]")).toBeNull();
    },
  );

  for (const service of WECHAT_SERVICES) {
    it("copies the full " + service.title + " URL using Clipboard API", async () => {
      const writeText = vi.fn().mockResolvedValue(undefined);
      clipboard({ writeText });
      renderAt(routeForService(service));
      fireEvent.click(await screen.findByRole("button", { name: "复制" + service.title + "链接" }));
      expect(await screen.findByRole("status")).toHaveTextContent(
        "链接已复制，请在微信内粘贴打开。",
      );
      expect(writeText).toHaveBeenCalledWith(service.url);
    });

    it("copies the full " + service.title + " URL through the selection fallback", async () => {
      clipboard(undefined);
      const execCommand = vi.fn(() => {
        expect(document.querySelector("textarea")?.value).toBe(service.url);
        return true;
      });
      fallback(execCommand);
      renderAt(routeForService(service));
      fireEvent.click(await screen.findByRole("button", { name: "复制" + service.title + "链接" }));
      expect(await screen.findByRole("status")).toHaveTextContent("链接已复制");
      expect(execCommand).toHaveBeenCalledWith("copy");
      expect(document.querySelector("textarea")).toBeNull();
    });

    it(
      "keeps the full " + service.title + " URL selectable when both copy methods fail",
      async () => {
        clipboard({ writeText: vi.fn().mockRejectedValue(new Error("denied")) });
        fallback(vi.fn().mockReturnValue(false));
        renderAt(routeForService(service));
        fireEvent.click(
          await screen.findByRole("button", { name: "复制" + service.title + "链接" }),
        );
        expect(await screen.findByRole("status")).toHaveTextContent(
          "无法自动复制，请长按或选择下方完整链接手动复制。",
        );
        expect(screen.queryByText("链接已复制，请在微信内粘贴打开。")).not.toBeInTheDocument();
        expect(screen.getByLabelText(service.title + "完整链接")).toHaveTextContent(service.url);
      },
    );
  }

  it("sends only a parameterless open command and suppresses duplicate clicks", async () => {
    vi.mocked(invoke).mockImplementation(async (command) =>
      command === "open_network_fee_portal" ? new Promise(() => {}) : defaultCommand(command),
    );
    renderAt("/campus-payments/network");
    const open = await screen.findByRole("button", { name: "打开网费官网" });
    fireEvent.click(open);
    fireEvent.click(open);
    const calls = vi
      .mocked(invoke)
      .mock.calls.filter(([command]) => command === "open_network_fee_portal");
    expect(calls).toEqual([["open_network_fee_portal"]]);
    expect(open).toBeDisabled();
    // 可见文案在自定义元素的 light DOM 里（shadow 内的 <slot> 承接它），
    // 因此断言宿主元素的文本而不是 shadow 内的按钮。
    expect(hostOf(open)).toHaveTextContent("正在打开网费官网…");
  });

  it("shows a recoverable open failure without exposing raw errors and permits retry", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "open_network_fee_portal") throw new Error("do-not-display-native-details");
      return defaultCommand(command);
    });
    renderAt("/campus-payments/network");
    const open = await screen.findByRole("button", { name: "打开网费官网" });
    fireEvent.click(open);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "无法打开网费官网，请检查网络或重新认证后重试",
    );
    expect(screen.queryByText(/do-not-display/)).not.toBeInTheDocument();
    expect(open).toBeEnabled();
    fireEvent.click(open);
    await waitFor(() =>
      expect(
        vi.mocked(invoke).mock.calls.filter(([command]) => command === "open_network_fee_portal"),
      ).toHaveLength(2),
    );
  });

  it("reads a one-shot failure notice only once even when StrictMode replays mounting", async () => {
    let resolveNotice!: (notice: NetworkFeePortalNotice | null) => void;
    const notice = new Promise<NetworkFeePortalNotice | null>((resolve) => {
      resolveNotice = resolve;
    });
    vi.mocked(invoke).mockImplementation(async (command) =>
      command === "consume_network_fee_portal_notice" ? notice : defaultCommand(command),
    );
    renderAt("/campus-payments/network", true);
    await screen.findByRole("heading", { name: "校园卡/电费/网费" });
    act(() => resolveNotice({ code: "loadTimeout", message: "官网加载超时，请重试。" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("官网加载超时，请重试。");
    expect(
      vi
        .mocked(invoke)
        .mock.calls.filter(([command]) => command === "consume_network_fee_portal_notice"),
    ).toHaveLength(1);
  });

  it("does not restore an old delayed notice after a new open request", async () => {
    let resolveNotice!: (notice: NetworkFeePortalNotice | null) => void;
    const notice = new Promise<NetworkFeePortalNotice | null>((resolve) => {
      resolveNotice = resolve;
    });
    vi.mocked(invoke).mockImplementation(async (command) =>
      command === "consume_network_fee_portal_notice" ? notice : defaultCommand(command),
    );
    renderAt("/campus-payments/network");
    fireEvent.click(await screen.findByRole("button", { name: "打开网费官网" }));
    await act(async () => resolveNotice({ code: "loginTimeout", message: "上一次登录超时" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("shows usage records by default and keeps current electricity separate", async () => {
    const options = [
      {
        id: "yuehai",
        name: "北校区（粤海校区）",
        areas: [
          {
            id: "yuehai-main",
            name: "粤海校区宿舍",
            campusId: "yuehai",
            provider: "sims",
            buildings: [{ id: "b1", name: "B1" }],
          },
        ],
      },
    ];
    const queryResult = {
      campusId: "yuehai",
      areaId: "yuehai-main",
      buildingName: "B1",
      roomName: "601",
      beginDate: "2026-09-11",
      endDate: "2026-09-17",
      currentRemaining: 12.34,
      readings: [
        {
          date: "2026-09-17",
          roomName: "601",
          remaining: 12.34,
          totalUsed: 8.5,
          totalPurchased: 20,
          dailyUsed: 1.2,
          dailyPurchased: 0,
        },
      ],
    };
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_electricity_options") return options;
      if (command === "get_electricity_settings") return null;
      if (command === "save_electricity_settings") return null;
      if (command === "query_electricity") return queryResult;
      return defaultCommand(command);
    });

    renderAt("/campus-payments/electricity");
    await screen.findByRole("heading", { name: "查电量" });
    await screen.findByText("北校区（粤海校区）");
    const campusSelect = screen.getByRole("combobox", { name: "校区" });
    const select = (campusSelect.getRootNode() as ShadowRoot).host || campusSelect;
    fireEvent(select, new CustomEvent("change", { detail: { value: "yuehai" } }));
    fireEvent.change(screen.getByRole("textbox", { name: "宿舍号" }), {
      target: { value: "601" },
    });
    fireEvent.click(screen.getByRole("button", { name: "查询电费" }));

    expect(await screen.findByRole("heading", { name: "用电记录" })).toBeInTheDocument();
    expect(screen.getByLabelText("当前电量")).toHaveTextContent("12.34 度");
    expect(screen.queryByRole("combobox", { name: "校区" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "修改宿舍配置" }));
    expect(screen.getByRole("combobox", { name: "校区" })).toBeInTheDocument();
    expect(screen.getByRole("grid", { name: "用电记录" })).toHaveTextContent("2026-09-17");
    expect(screen.getByRole("grid", { name: "用电记录" })).toHaveTextContent("当日用电");
    expect(invoke).toHaveBeenCalledWith("query_electricity", {
      input: expect.objectContaining({ buildingId: "b1", roomName: "601" }),
    });
  });

  it("automatically refreshes the saved room when entering the electricity page", async () => {
    const saved = {
      campusId: "yuehai",
      areaId: "yuehai-main",
      buildingId: "b1",
      buildingName: "B1",
      roomName: "601",
    };
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_electricity_options") {
        return [
          {
            id: "yuehai",
            name: "北校区（粤海校区）",
            areas: [
              {
                id: "yuehai-main",
                name: "粤海校区宿舍",
                campusId: "yuehai",
                provider: "sims",
                buildings: [{ id: "b1", name: "B1" }],
              },
            ],
          },
        ];
      }
      if (command === "get_electricity_settings") return saved;
      if (command === "query_electricity") {
        return {
          campusId: "yuehai",
          areaId: "yuehai-main",
          buildingName: "B1",
          roomName: "601",
          beginDate: "2026-09-12",
          endDate: "2026-09-18",
          currentRemaining: 9.87,
          readings: [],
        };
      }
      return defaultCommand(command);
    });

    renderAt("/campus-payments/electricity");
    expect(await screen.findByLabelText("当前电量")).toHaveTextContent("9.87 度");
    expect(screen.queryByRole("button", { name: /校区/ })).not.toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("query_electricity", {
      input: expect.objectContaining({ buildingId: "b1", roomName: "601" }),
    });
  });

  it("starts the saved-room query after the page is rendered", async () => {
    const saved = {
      campusId: "yuehai",
      areaId: "yuehai-main",
      buildingId: "b1",
      buildingName: "B1",
      roomName: "601",
    };
    let resolveQuery!: (value: unknown) => void;
    const query = new Promise((resolve) => {
      resolveQuery = resolve;
    });
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_electricity_options") {
        return [
          {
            id: "yuehai",
            name: "北校区（粤海校区）",
            areas: [
              {
                id: "yuehai-main",
                name: "粤海校区宿舍",
                campusId: "yuehai",
                provider: "sims",
                buildings: [{ id: "b1", name: "B1" }],
              },
            ],
          },
        ];
      }
      if (command === "get_electricity_settings") return saved;
      if (command === "query_electricity") return query;
      return defaultCommand(command);
    });

    renderAt("/campus-payments/electricity", true);
    expect(await screen.findByRole("heading", { name: "查电量" })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("button", { name: /查询中/ })).toBeDisabled());
    expect(screen.queryByLabelText("当前电量")).not.toBeInTheDocument();
    expect(
      vi.mocked(invoke).mock.calls.filter(([command]) => command === "query_electricity"),
    ).toHaveLength(1);

    resolveQuery({
      campusId: "yuehai",
      areaId: "yuehai-main",
      buildingName: "B1",
      roomName: "601",
      beginDate: "2026-09-12",
      endDate: "2026-09-18",
      currentRemaining: 9.87,
      readings: [],
    });
    expect(await screen.findByLabelText("当前电量")).toHaveTextContent("9.87 度");
  });

  it("renders the electricity page while its initial data is still loading", async () => {
    let resolveOptions!: (value: unknown) => void;
    let resolveSettings!: (value: unknown) => void;
    const options = new Promise((resolve) => {
      resolveOptions = resolve;
    });
    const settings = new Promise((resolve) => {
      resolveSettings = resolve;
    });
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_electricity_options") return options;
      if (command === "get_electricity_settings") return settings;
      return defaultCommand(command);
    });

    renderAt("/campus-payments/electricity");
    expect(await screen.findByRole("heading", { name: "查电量" })).toBeInTheDocument();
    expect(screen.getByText("正在加载宿舍选项…")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "查询电费" })).toBeDisabled();

    resolveOptions([]);
    resolveSettings(null);
    await waitFor(() => expect(screen.queryByText("正在加载宿舍选项…")).not.toBeInTheDocument());
  });
});
