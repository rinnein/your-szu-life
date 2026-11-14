/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { toast } from "./ui";
import {
  initialLogicalEntries,
  isChildDocument,
  LogicalNavigationProvider,
  useLogicalNavigation,
} from "./logical-navigation";
import { useRouter } from "@tanstack/react-router";
import { supportsMultipleWindows } from "@tauri-apps/api/app";
import { WebviewWindow, getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

vi.mock("@tanstack/react-router", () => ({
  useRouter: vi.fn(),
}));

vi.mock("@tauri-apps/api/app", () => ({
  supportsMultipleWindows: vi.fn(),
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  WebviewWindow: vi.fn(),
  getCurrentWebviewWindow: vi.fn(),
}));

describe("logical navigation entry stack", () => {
  it("seeds a direct feature link with the workbench parent", () => {
    expect(initialLogicalEntries("https://tauri.localhost/settings?tab=account#security")).toEqual([
      "/",
      "/settings?tab=account#security",
    ]);
  });

  it("keeps a child Activity as its own logical root", () => {
    const href = "https://tauri.localhost/settings?tab=account&__szu_window=child#security";
    expect(isChildDocument(href)).toBe(true);
    expect(initialLogicalEntries(href)).toEqual(["/settings?tab=account#security"]);
  });

  it("keeps the workbench as the root entry", () => {
    expect(initialLogicalEntries("https://tauri.localhost/")).toEqual(["/"]);
  });
});

it("includes the study hub for an OJ deep link", () => {
  expect(initialLogicalEntries("https://tauri.localhost/study/oj?site=szu-67")).toEqual([
    "/",
    "/study",
    "/study/oj?site=szu-67",
  ]);
  expect(initialLogicalEntries("https://tauri.localhost/study/oj?__szu_window=child")).toEqual([
    "/study/oj",
  ]);
});

it("derives the parent chain from the nested route path", () => {
  expect(initialLogicalEntries("https://tauri.localhost/study/uooc?tab=tasks")).toEqual([
    "/",
    "/study",
    "/study/uooc?tab=tasks",
  ]);
  expect(initialLogicalEntries("https://tauri.localhost/life")).toEqual(["/", "/life"]);
  expect(initialLogicalEntries("https://tauri.localhost/life/campus-payments/electricity")).toEqual(
    ["/", "/life", "/life/campus-payments", "/life/campus-payments/electricity"],
  );
});

it("restores enrollment parents for nested, legacy and authentication return links", () => {
  for (const path of [
    "/study/enrollment/tasks",
    "/study/enrollment?tab=tasks",
    "/study/enrollment/login?returnTo=%2Fstudy%2Fenrollment%2Ftasks",
  ]) {
    expect(initialLogicalEntries(`https://tauri.localhost${path}`)).toEqual([
      "/",
      "/study",
      "/study/enrollment",
      path,
    ]);
  }
  expect(initialLogicalEntries("https://tauri.localhost/study/enrollment/login")).toEqual([
    "/",
    "/study",
    "/study/enrollment/login",
  ]);
});

function TestNavConsumer() {
  const nav = useLogicalNavigation();
  return (
    <button type="button" onClick={() => void nav.push("/fail")}>
      Navigate
    </button>
  );
}

describe("LogicalNavigationProvider toast error", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it("triggers toast.danger when push fails", async () => {
    const dangerSpy = vi.spyOn(toast, "danger").mockReturnValue("toast-id");
    vi.mocked(useRouter).mockReturnValue({
      history: { canGoBack: () => false, back: vi.fn() },
      navigate: vi.fn().mockRejectedValue(new Error("navigation failed")),
    } as unknown as ReturnType<typeof useRouter>);

    render(
      <LogicalNavigationProvider>
        <TestNavConsumer />
      </LogicalNavigationProvider>,
    );

    const button = screen.getByRole("button", { name: "Navigate" });
    button.click();

    await vi.waitFor(() => {
      expect(dangerSpy).toHaveBeenCalledWith("页面未打开", {
        description: "无法打开独立页面，请重试；当前页面已保留。",
      });
    });
  });
});

describe("desktop navigation and pin", () => {
  const createdWindows: { label: string; options: Record<string, unknown> }[] = [];

  // 用 function 声明实现：`new WebviewWindow(...)` 需要真实的构造语义。
  function mockCreatedWindow(mode: "created" | "error") {
    vi.mocked(WebviewWindow).mockImplementation(function MockWindow(
      label: string,
      options: Record<string, unknown>,
    ) {
      createdWindows.push({ label, options });
      return {
        once: (event: string, callback: () => void) => {
          if (event === `tauri://${mode}`) queueMicrotask(callback);
          return Promise.resolve(() => {});
        },
      } as unknown as WebviewWindow;
    } as unknown as typeof WebviewWindow);
  }

  function mockRouter({ href = "/" }: { href?: string } = {}) {
    const navigate = vi.fn().mockResolvedValue(undefined);
    const back = vi.fn();
    vi.mocked(useRouter).mockReturnValue({
      history: { canGoBack: () => true, back },
      navigate,
      state: { location: { href } },
    } as unknown as ReturnType<typeof useRouter>);
    return { navigate, back };
  }

  beforeEach(() => {
    vi.clearAllMocks();
    createdWindows.length = 0;
    (window as { __TAURI_INTERNALS__?: object }).__TAURI_INTERNALS__ = {};
    vi.mocked(supportsMultipleWindows).mockResolvedValue(true);
    vi.mocked(getCurrentWebviewWindow).mockReturnValue({
      activityName: vi.fn(() => Promise.resolve("main")),
      onCloseRequested: vi.fn(() => Promise.resolve(() => {})),
    } as unknown as ReturnType<typeof getCurrentWebviewWindow>);
  });

  afterEach(() => {
    cleanup();
    delete (window as { __TAURI_INTERNALS__?: object; SzuApplicationBack?: object })
      .SzuApplicationBack;
    delete (window as { __TAURI_INTERNALS__?: object }).__TAURI_INTERNALS__;
  });

  function PushConsumer({ path }: { path: string }) {
    const nav = useLogicalNavigation();
    return (
      <button type="button" onClick={() => void nav.push(path)}>
        Navigate
      </button>
    );
  }

  function PinConsumer() {
    const nav = useLogicalNavigation();
    return (
      <button type="button" onClick={() => void nav.pin()}>
        Pin
      </button>
    );
  }

  it("desktop push navigates inside the window", async () => {
    mockCreatedWindow("created");
    const { navigate } = mockRouter();
    render(
      <LogicalNavigationProvider>
        <PushConsumer path="/study/oj?site=szu-67" />
      </LogicalNavigationProvider>,
    );

    screen.getByRole("button", { name: "Navigate" }).click();

    await vi.waitFor(() => expect(navigate).toHaveBeenCalledWith({ to: "/study/oj?site=szu-67" }));
    expect(createdWindows).toHaveLength(0);
  });

  it("android push keeps opening a feature window", async () => {
    (window as { SzuApplicationBack?: object }).SzuApplicationBack = { postMessage: vi.fn() };
    mockCreatedWindow("created");
    const { navigate } = mockRouter();
    render(
      <LogicalNavigationProvider>
        <PushConsumer path="/study/oj?site=szu-67" />
      </LogicalNavigationProvider>,
    );

    screen.getByRole("button", { name: "Navigate" }).click();

    await vi.waitFor(() => expect(createdWindows).toHaveLength(1));
    const { label, options } = createdWindows[0];
    expect(label).toMatch(/^feature-/);
    expect(options.url).toBe("/study/oj?site=szu-67&__szu_window=child");
    expect(options.activityName).toBe("FeatureActivity");
    // 窗口创建成功时不应再走窗口内导航。
    expect(navigate).not.toHaveBeenCalled();
  });

  it("desktop pin opens the current page in a feature window and goes back", async () => {
    mockCreatedWindow("created");
    const { navigate, back } = mockRouter({ href: "/study/uooc?tab=tasks" });
    render(
      <LogicalNavigationProvider>
        <PinConsumer />
      </LogicalNavigationProvider>,
    );

    screen.getByRole("button", { name: "Pin" }).click();

    await vi.waitFor(() => expect(createdWindows).toHaveLength(1));
    const { label, options } = createdWindows[0];
    expect(label).toMatch(/^feature-/);
    expect(options.url).toBe("/study/uooc?tab=tasks&__szu_window=child");
    // 主窗口退回上一级逻辑页面（back），而不是做窗口内 push。
    await vi.waitFor(() => expect(back).toHaveBeenCalled());
    expect(navigate).not.toHaveBeenCalled();
  });

  it("desktop pin falls back to toast when window creation fails", async () => {
    const dangerSpy = vi.spyOn(toast, "danger").mockReturnValue("toast-id");
    mockCreatedWindow("error");
    const { back } = mockRouter({ href: "/study/uooc" });
    render(
      <LogicalNavigationProvider>
        <PinConsumer />
      </LogicalNavigationProvider>,
    );

    screen.getByRole("button", { name: "Pin" }).click();

    await vi.waitFor(() => {
      expect(dangerSpy).toHaveBeenCalledWith("页面未固定", {
        description: "无法以独立窗口打开当前页面，请重试；当前页面已保留。",
      });
    });
    expect(back).not.toHaveBeenCalled();
  });

  it("child window pin is a no-op", async () => {
    window.history.replaceState(null, "", "?__szu_window=child");
    mockCreatedWindow("created");
    const { back } = mockRouter({ href: "/study/uooc?__szu_window=child" });
    render(
      <LogicalNavigationProvider>
        <PinConsumer />
      </LogicalNavigationProvider>,
    );

    screen.getByRole("button", { name: "Pin" }).click();

    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(createdWindows).toHaveLength(0);
    expect(back).not.toHaveBeenCalled();
    window.history.replaceState(null, "", "/");
  });
});
