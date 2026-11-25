/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { useRouter } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { supportsMultipleWindows } from "@tauri-apps/api/app";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { LogicalNavigationProvider } from "./logical-navigation";
import { DeepLinkNavigation } from "./DeepLinkNavigation";
import { setupDeepLinkListener } from "./deep-link";
import { pairLoginAcceptUrl } from "./pair-login/api";

vi.mock("@tanstack/react-router", () => ({
  useRouter: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

vi.mock("@tauri-apps/api/app", () => ({
  supportsMultipleWindows: vi.fn(),
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  WebviewWindow: vi.fn(),
  getCurrentWebviewWindow: vi.fn(),
}));

vi.mock("./deep-link", () => ({
  setupDeepLinkListener: vi.fn(async () => () => {}),
}));

vi.mock("./pair-login/api", () => ({
  pairLoginAcceptUrl: vi.fn(),
}));

type TrayEventHandler = (event: { payload?: { route?: string } }) => void;

describe("DeepLinkNavigation tray bridge", () => {
  let trayHandler: TrayEventHandler | undefined;
  let deepLinkHandler: ((route: string) => void) | undefined;
  let navigate: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.clearAllMocks();
    trayHandler = undefined;
    deepLinkHandler = undefined;
    (window as { __TAURI_INTERNALS__?: object }).__TAURI_INTERNALS__ = {};
    vi.mocked(listen).mockImplementation(async (...args: Parameters<typeof listen>) => {
      trayHandler = args[1] as TrayEventHandler;
      return () => {};
    });
    vi.mocked(setupDeepLinkListener).mockImplementation(async (callback) => {
      deepLinkHandler = callback;
      return () => {};
    });
    vi.mocked(supportsMultipleWindows).mockResolvedValue(true);
    vi.mocked(getCurrentWebviewWindow).mockReturnValue({
      onCloseRequested: vi.fn(() => Promise.resolve(() => {})),
    } as unknown as ReturnType<typeof getCurrentWebviewWindow>);
    navigate = vi.fn().mockResolvedValue(undefined);
    vi.mocked(invoke).mockImplementation(async () => ({ Authenticated: { name: "test" } }));
    vi.mocked(pairLoginAcceptUrl).mockResolvedValue(undefined);
    vi.mocked(useRouter).mockReturnValue({
      history: { canGoBack: () => true, back: vi.fn() },
      navigate,
      state: { location: { href: "/" } },
    } as unknown as ReturnType<typeof useRouter>);
  });

  afterEach(() => {
    cleanup();
    delete (window as { __TAURI_INTERNALS__?: object }).__TAURI_INTERNALS__;
  });

  it("navigates the main window on tray events", async () => {
    render(
      <LogicalNavigationProvider>
        <DeepLinkNavigation />
      </LogicalNavigationProvider>,
    );

    await vi.waitFor(() => expect(trayHandler).toBeDefined());
    trayHandler?.({ payload: { route: "/study/uooc?tab=exams&subtab=homework" } });

    await vi.waitFor(() =>
      expect(navigate).toHaveBeenCalledWith({ to: "/study/uooc?tab=exams&subtab=homework" }),
    );
  });

  it("replaces the current page when a deep link targets the app root", async () => {
    render(
      <LogicalNavigationProvider>
        <DeepLinkNavigation />
      </LogicalNavigationProvider>,
    );

    await vi.waitFor(() => expect(deepLinkHandler).toBeDefined());
    deepLinkHandler?.("/");

    // 首页必须替换当前页面，而不是 push 出新的一层（Android 上会变成独立窗口）。
    await vi.waitFor(() =>
      expect(navigate).toHaveBeenCalledWith({
        to: "/",
        replace: true,
        resetScroll: true,
      }),
    );
  });

  it("pushes feature deep links as new logical pages", async () => {
    render(
      <LogicalNavigationProvider>
        <DeepLinkNavigation />
      </LogicalNavigationProvider>,
    );

    await vi.waitFor(() => expect(deepLinkHandler).toBeDefined());
    deepLinkHandler?.("/study/uooc");

    await vi.waitFor(() => expect(navigate).toHaveBeenCalledWith({ to: "/study/uooc" }));
  });

  it("ignores payloads without an application route", async () => {
    render(
      <LogicalNavigationProvider>
        <DeepLinkNavigation />
      </LogicalNavigationProvider>,
    );

    await vi.waitFor(() => expect(trayHandler).toBeDefined());
    trayHandler?.({ payload: { route: "https://evil.example.com/" } });
    trayHandler?.({ payload: {} });

    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(navigate).not.toHaveBeenCalled();
  });

  it("accepts a pair invite into Rust state without putting the secret in the route", async () => {
    render(
      <LogicalNavigationProvider>
        <DeepLinkNavigation />
      </LogicalNavigationProvider>,
    );

    await vi.waitFor(() => expect(deepLinkHandler).toBeDefined());
    const invite = "/pair-login?secret=" + "a".repeat(64);
    deepLinkHandler?.(invite);

    await vi.waitFor(() => expect(pairLoginAcceptUrl).toHaveBeenCalledWith(invite));
    await vi.waitFor(() => expect(navigate).toHaveBeenCalledWith({ to: "/pair-login" }));
    expect(JSON.stringify(vi.mocked(navigate).mock.calls)).not.toContain("secret=");
  });

  it("refuses to open the authorize page when the device is not signed in", async () => {
    vi.mocked(invoke).mockImplementation(async () => ({ Unauthenticated: null }));
    render(
      <LogicalNavigationProvider>
        <DeepLinkNavigation />
      </LogicalNavigationProvider>,
    );

    await vi.waitFor(() => expect(deepLinkHandler).toBeDefined());
    deepLinkHandler?.("/pair-login?secret=" + "b".repeat(64));

    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(pairLoginAcceptUrl).not.toHaveBeenCalled();
    expect(navigate).not.toHaveBeenCalled();
  });
});
