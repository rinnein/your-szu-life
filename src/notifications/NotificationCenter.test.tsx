/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { onAction } from "@tauri-apps/plugin-notification";
import { toast } from "../ui";
import { NotificationCenter, type InAppNotificationPayload } from "./NotificationCenter";

const { nav, router } = vi.hoisted(() => ({
  nav: { push: vi.fn(), replace: vi.fn(), back: vi.fn() },
  router: { state: { location: { pathname: "/" } } },
}));

vi.mock("../logical-navigation", () => ({
  useLogicalNavigation: () => nav,
  isChildDocument: () => false,
}));

vi.mock("@tanstack/react-router", () => ({
  useRouter: () => router,
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({ onAction: vi.fn() }));

type NotificationActionEvent = {
  actionId: string;
  inputValue: string | null;
  notification: { actionTypeId?: string; extra?: Record<string, unknown> } | null;
};

describe("NotificationCenter", () => {
  let eventCallback: ((event: { payload: InAppNotificationPayload }) => void) | null = null;
  let actionCallback: ((event: NotificationActionEvent) => void) | null = null;

  beforeEach(() => {
    vi.clearAllMocks();
    (window as any).__TAURI_INTERNALS__ = {};
    router.state.location.pathname = "/";
    eventCallback = null;
    actionCallback = null;

    vi.mocked(listen).mockImplementation(async (eventName: string, cb: any) => {
      if (eventName === "app://in-app-notification") eventCallback = cb;
      return () => undefined;
    });
    vi.mocked(onAction).mockImplementation(async (cb: any) => {
      actionCallback = cb;
      return { unregister: vi.fn() } as any;
    });
    vi.mocked(invoke).mockResolvedValue(undefined);
    vi.spyOn(toast, "info").mockImplementation(() => "info");
    vi.spyOn(toast, "warning").mockImplementation(() => "warning");
  });

  afterEach(() => {
    cleanup();
    delete (window as any).__TAURI_INTERNALS__;
  });

  async function renderCenter() {
    render(<NotificationCenter />);
    await waitFor(() => {
      expect(eventCallback).not.toBeNull();
      expect(actionCallback).not.toBeNull();
    });
  }

  it("shows an info snackbar and navigates from its action", async () => {
    await renderCenter();

    eventCallback!({
      payload: {
        category: "uooc",
        title: "UOOC 测验需要处理",
        body: "计算机网络课程有未完成的测验",
        route: "/study/uooc?tab=tasks",
        actionLabel: "查看测验",
      },
    });

    expect(toast.info).toHaveBeenCalledWith(
      "UOOC 测验需要处理",
      expect.objectContaining({
        description: "计算机网络课程有未完成的测验",
        actionLabel: "查看测验",
        onAction: expect.any(Function),
      }),
    );

    const options = vi.mocked(toast.info).mock.calls[0][1] as { onAction: () => void };
    options.onAction();
    expect(nav.push).toHaveBeenCalledWith("/study/uooc?tab=tasks");
  });

  it("uses a warning snackbar for electricity and network categories", async () => {
    await renderCenter();

    eventCallback!({
      payload: {
        category: "electricity",
        title: "当前剩余电量：8.50 度",
        body: "宿舍电量不足，请及时充值。",
        route: "/life/campus-payments/recharge",
      },
    });

    expect(toast.warning).toHaveBeenCalledWith(
      "当前剩余电量：8.50 度",
      expect.objectContaining({ actionLabel: "查看" }),
    );
    expect(toast.info).not.toHaveBeenCalled();
  });

  it("routes notification taps through the payload route", async () => {
    await renderCenter();

    actionCallback!({
      actionId: "tap",
      inputValue: null,
      notification: { extra: { route: "/study/oj?site=s1&contest=42" } },
    });

    expect(nav.push).toHaveBeenCalledWith("/study/oj?site=s1&contest=42");
  });

  it("replaces instead of pushing when the target page is already open", async () => {
    router.state.location.pathname = "/study/uooc";
    await renderCenter();

    actionCallback!({
      actionId: "tap",
      inputValue: null,
      notification: { extra: { uoocCourse: "12345" } },
    });

    expect(nav.replace).toHaveBeenCalledWith("/study/uooc?tab=tasks&course=12345");
    expect(nav.push).not.toHaveBeenCalled();
  });

  it("resolves a scheduled OJ reminder against the current snapshot", async () => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === "get_oj_snapshot") {
        return { sites: [{ id: "s1", username: "u1", enabled: true }] };
      }
      return undefined;
    });
    await renderCenter();

    actionCallback!({
      actionId: "tap",
      inputValue: null,
      notification: {
        extra: {
          ojNotice: {
            id: "s1:u1:42",
            siteId: "s1",
            account: "u1",
            contestId: "42",
            title: "算法测验",
            message: "测验将在 1 小时后截止",
            timestamp: 0,
            read: false,
          },
        },
      },
    });

    await waitFor(() => {
      expect(nav.push).toHaveBeenCalledWith("/study/oj?site=s1&contest=42");
    });
  });
});
