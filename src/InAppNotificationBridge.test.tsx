/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "./ui";
import { InAppNotificationBridge, type InAppNotificationPayload } from "./InAppNotificationBridge";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

const mockPush = vi.fn();
vi.mock("./logical-navigation", () => ({
  useLogicalNavigation: () => ({
    push: mockPush,
    replace: vi.fn(),
    back: vi.fn(),
  }),
}));

describe("InAppNotificationBridge component", () => {
  let eventCallback: ((event: { payload: InAppNotificationPayload }) => void) | null = null;

  beforeEach(() => {
    vi.clearAllMocks();
    (window as any).__TAURI_INTERNALS__ = {};
    eventCallback = null;
    vi.mocked(listen).mockImplementation(async (eventName: string, cb: any) => {
      if (eventName === "app://in-app-notification") {
        eventCallback = cb;
      }
      return () => undefined;
    });
    vi.spyOn(toast, "info").mockImplementation(() => "id-1");
    vi.spyOn(toast, "warning").mockImplementation(() => "id-2");
  });

  afterEach(() => {
    cleanup();
    delete (window as any).__TAURI_INTERNALS__;
  });

  it("reports focus state to Rust on mount and window focus/blur", async () => {
    vi.spyOn(document, "hasFocus").mockReturnValue(true);
    render(<InAppNotificationBridge />);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_application_focus", {
        focused: true,
      });
    });

    vi.spyOn(document, "hasFocus").mockReturnValue(false);
    window.dispatchEvent(new Event("blur"));
    expect(invoke).toHaveBeenCalledWith("set_application_focus", {
      focused: false,
    });
  });

  it("shows in-app toast when receiving notification event", async () => {
    render(<InAppNotificationBridge />);

    await waitFor(() => {
      expect(listen).toHaveBeenCalledWith("app://in-app-notification", expect.any(Function));
    });

    expect(eventCallback).not.toBeNull();

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
  });

  it("shows warning toast for failure and electricity categories", async () => {
    render(<InAppNotificationBridge />);

    await waitFor(() => {
      expect(listen).toHaveBeenCalledWith("app://in-app-notification", expect.any(Function));
    });

    eventCallback!({
      payload: {
        category: "electricity",
        title: "当前剩余电量：8.50 度",
        body: "宿舍电量不足，请及时充值。",
        route: "/life/campus-payments/recharge",
        actionLabel: "前往充值",
      },
    });

    expect(toast.warning).toHaveBeenCalledWith(
      "当前剩余电量：8.50 度",
      expect.objectContaining({
        description: "宿舍电量不足，请及时充值。",
        actionLabel: "前往充值",
      }),
    );
  });
});
