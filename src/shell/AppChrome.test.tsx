/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vite-plus/test";
import type { ReactNode } from "react";

vi.mock("./native-chrome", () => ({ useNativeChromeColors: () => undefined }));
vi.mock("./AppNavigation", () => ({ AppNavigation: () => <nav data-testid="app-nav" /> }));
vi.mock("../motion/PageTransition", () => ({
  PageTransition: ({ children }: { children: ReactNode }) => <>{children}</>,
}));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  WebviewWindow: vi.fn(),
  getCurrentWebviewWindow: vi.fn(() => ({
    onCloseRequested: vi.fn(() => Promise.resolve(() => {})),
    setTitle: vi.fn(() => Promise.resolve()),
  })),
}));

import { AppChrome } from "./AppChrome";

afterEach(() => {
  cleanup();
  window.history.replaceState(null, "", "/");
  delete (window as { __TAURI_INTERNALS__?: object }).__TAURI_INTERNALS__;
  delete (window as { SzuApplicationBack?: object }).SzuApplicationBack;
});

function renderApp() {
  return render(
    <AppChrome>
      <div>content</div>
    </AppChrome>,
  );
}

function setEnvironment({ desktop, child }: { desktop: boolean; child: boolean }) {
  if (desktop) (window as { __TAURI_INTERNALS__?: object }).__TAURI_INTERNALS__ = {};
  if (!desktop)
    (window as { SzuApplicationBack?: object }).SzuApplicationBack = {
      postMessage: vi.fn(),
    };
  if (child) window.history.replaceState(null, "", "?__szu_window=child");
}

describe("AppChrome bottom navigation", () => {
  it("keeps the navigation bar in the desktop main window", () => {
    setEnvironment({ desktop: true, child: false });
    renderApp();
    expect(screen.getByTestId("app-nav")).toBeInTheDocument();
    expect(document.querySelector(".app-frame")).not.toHaveAttribute("data-child-window");
  });

  it("hides the navigation bar in desktop child windows", () => {
    setEnvironment({ desktop: true, child: true });
    renderApp();
    expect(screen.queryByTestId("app-nav")).not.toBeInTheDocument();
    expect(document.querySelector(".app-frame")).toHaveAttribute("data-child-window");
  });

  it("keeps the navigation bar in android feature activities", () => {
    setEnvironment({ desktop: false, child: true });
    renderApp();
    expect(screen.getByTestId("app-nav")).toBeInTheDocument();
    expect(document.querySelector(".app-frame")).not.toHaveAttribute("data-child-window");
  });
});
