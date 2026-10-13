/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { toast } from "@heroui/react";
import { UpdateNotice } from "./UpdateNotice";
import { useAppContent } from "./context";
import { useRouterState } from "@tanstack/react-router";
import { useLogicalNavigation } from "../logical-navigation";
import type { AppContentSnapshot } from "./types";

vi.mock("./context", () => ({
  useAppContent: vi.fn(),
}));

vi.mock("@tanstack/react-router", () => ({
  useRouterState: vi.fn(),
}));

vi.mock("../logical-navigation", () => ({
  useLogicalNavigation: vi.fn(),
}));

function makeSnapshot(updateAvailable: boolean, latestVer?: string): AppContentSnapshot {
  return {
    revision: 1,
    version: "0.1.0",
    configuration: { status: "ready", message: null },
    repository: null,
    repositoryMessage: null,
    refreshing: false,
    nextAutomaticCheckAt: null,
    announcements: {
      data: null,
      lastAttemptAt: null,
      lastSuccessAt: null,
      source: null,
      fromCache: false,
      error: null,
    },
    update: {
      data: latestVer
        ? {
            latest_ver: latestVer,
            latest_ver_url: "https://example.com/download",
            changelog: "修复已知问题",
          }
        : null,
      lastAttemptAt: null,
      lastSuccessAt: null,
      source: "primary",
      fromCache: false,
      error: null,
    },
    updateAvailable,
  };
}

describe("UpdateNotice with toast", () => {
  const pushMock = vi.fn();

  let currentPath = "/";

  beforeEach(() => {
    vi.clearAllMocks();
    currentPath = "/";
    vi.mocked(useRouterState).mockImplementation(((opts?: {
      select?: (state: unknown) => unknown;
    }) => {
      const state = { location: { pathname: currentPath } };
      return opts?.select ? opts.select(state) : state;
    }) as never);
    vi.mocked(useLogicalNavigation).mockReturnValue({
      push: pushMock,
      replace: vi.fn(),
      back: vi.fn(),
      canBack: () => false,
    });
  });

  afterEach(() => {
    cleanup();
  });

  it("triggers toast.info when a new update is available", () => {
    const toastSpy = vi.spyOn(toast, "info").mockReturnValue("mock-toast-id");
    currentPath = "/";
    vi.mocked(useAppContent).mockReturnValue({
      snapshot: makeSnapshot(true, "0.2.0"),
      refreshing: false,
      refresh: vi.fn(),
    });

    render(<UpdateNotice />);

    expect(toastSpy).toHaveBeenCalledWith(
      "发现新版本 0.2.0",
      expect.objectContaining({
        description: "查看更新日志，按需前往下载。",
        timeout: 0,
      }),
    );
  });

  it("does not trigger toast when on the about page", () => {
    const toastSpy = vi.spyOn(toast, "info");
    currentPath = "/about";
    vi.mocked(useAppContent).mockReturnValue({
      snapshot: makeSnapshot(true, "0.2.0"),
      refreshing: false,
      refresh: vi.fn(),
    });

    render(<UpdateNotice />);

    expect(toastSpy).not.toHaveBeenCalled();
  });

  it("navigates to about page and closes toast when action button is pressed", () => {
    const closeSpy = vi.spyOn(toast, "close");
    let actionHandler: (() => void) | undefined;
    vi.spyOn(toast, "info").mockImplementation((_title, opts) => {
      actionHandler = (opts?.actionProps as { onPress?: () => void })?.onPress;
      return "mock-toast-id";
    });
    currentPath = "/";
    vi.mocked(useAppContent).mockReturnValue({
      snapshot: makeSnapshot(true, "0.2.0"),
      refreshing: false,
      refresh: vi.fn(),
    });

    render(<UpdateNotice />);
    expect(actionHandler).toBeDefined();

    actionHandler?.();
    expect(closeSpy).toHaveBeenCalledWith("mock-toast-id");
    expect(pushMock).toHaveBeenCalledWith("/about");
  });

  it("closes toast on unmount", () => {
    const closeSpy = vi.spyOn(toast, "close");
    vi.spyOn(toast, "info").mockReturnValue("mock-toast-id");
    currentPath = "/";
    vi.mocked(useAppContent).mockReturnValue({
      snapshot: makeSnapshot(true, "0.2.0"),
      refreshing: false,
      refresh: vi.fn(),
    });

    const { unmount } = render(<UpdateNotice />);
    unmount();

    expect(closeSpy).toHaveBeenCalledWith("mock-toast-id");
  });
});
