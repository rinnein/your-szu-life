/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { useRouter } from "@tanstack/react-router";
import { listen } from "@tauri-apps/api/event";
import { supportsMultipleWindows } from "@tauri-apps/api/app";
import { WebviewWindow, getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
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
  const createdWindows: { label: string; options: Record<string, unknown> }[] = [];

  beforeEach(() => {
    vi.clearAllMocks();
    trayHandler = undefined;
    createdWindows.length = 0;
    (window as { __TAURI_INTERNALS__?: object }).__TAURI_INTERNALS__ = {};
    vi.mocked(listen).mockImplementation(async (...args: Parameters<typeof listen>) => {
      trayHandler = args[1] as TrayEventHandler;
      return () => {};
    });
    vi.mocked(supportsMultipleWindows).mockResolvedValue(true);
    // 用 function 声明实现：`new WebviewWindow(...)` 需要真实的构造语义。
    vi.mocked(WebviewWindow).mockImplementation(function MockWindow(
      label: string,
      options: Record<string, unknown>,
    ) {
      createdWindows.push({ label, options });
      return {
        once: (event: string, callback: () => void) => {
          if (event === "tauri://created") queueMicrotask(callback);
          return Promise.resolve(() => {});
        },
      } as unknown as WebviewWindow;
    } as unknown as typeof WebviewWindow);
    vi.mocked(getCurrentWebviewWindow).mockReturnValue({
      onCloseRequested: vi.fn(() => Promise.resolve(() => {})),
    } as unknown as ReturnType<typeof getCurrentWebviewWindow>);
    vi.mocked(useRouter).mockReturnValue({
      history: { canGoBack: () => false, back: vi.fn() },
      navigate: vi.fn(),
    } as unknown as ReturnType<typeof useRouter>);
  });

  afterEach(() => {
    cleanup();
    delete (window as { __TAURI_INTERNALS__?: object }).__TAURI_INTERNALS__;
  });

  it("opens the todo route in a standalone window on tray events", async () => {
    render(
      <LogicalNavigationProvider>
        <DeepLinkNavigation />
      </LogicalNavigationProvider>,
    );

    await vi.waitFor(() => expect(trayHandler).toBeDefined());
    trayHandler?.({ payload: { route: "/study/uooc?tab=exams&subtab=homework" } });

    await vi.waitFor(() => expect(createdWindows).toHaveLength(1));
    const { label, options } = createdWindows[0];
    expect(label).toMatch(/^feature-/);
    expect(options.url).toBe("/study/uooc?tab=exams&subtab=homework&__szu_window=child");
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
    expect(createdWindows).toHaveLength(0);
  });
});
