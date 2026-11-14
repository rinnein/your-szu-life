/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { useRouter } from "@tanstack/react-router";
import { listen } from "@tauri-apps/api/event";
import { supportsMultipleWindows } from "@tauri-apps/api/app";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { LogicalNavigationProvider } from "./logical-navigation";
import { DeepLinkNavigation } from "./DeepLinkNavigation";

vi.mock("@tanstack/react-router", () => ({
  useRouter: vi.fn(),
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

type TrayEventHandler = (event: { payload?: { route?: string } }) => void;

describe("DeepLinkNavigation tray bridge", () => {
  let trayHandler: TrayEventHandler | undefined;
  let navigate: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.clearAllMocks();
    trayHandler = undefined;
    (window as { __TAURI_INTERNALS__?: object }).__TAURI_INTERNALS__ = {};
    vi.mocked(listen).mockImplementation(async (...args: Parameters<typeof listen>) => {
      trayHandler = args[1] as TrayEventHandler;
      return () => {};
    });
    vi.mocked(supportsMultipleWindows).mockResolvedValue(true);
    vi.mocked(getCurrentWebviewWindow).mockReturnValue({
      onCloseRequested: vi.fn(() => Promise.resolve(() => {})),
    } as unknown as ReturnType<typeof getCurrentWebviewWindow>);
    navigate = vi.fn().mockResolvedValue(undefined);
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
});
