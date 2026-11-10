/** @vitest-environment jsdom */

import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { startChildWindowTitleSync } from "./child-window-title";

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: vi.fn(),
}));

function setTitleMock() {
  const setTitle = vi.fn(async () => {});
  vi.mocked(getCurrentWebviewWindow).mockReturnValue({
    setTitle,
  } as unknown as ReturnType<typeof getCurrentWebviewWindow>);
  return setTitle;
}

describe("child window title sync", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    window.history.replaceState(null, "", "?__szu_window=child");
    if (!document.head.querySelector("title")) {
      const title = document.createElement("title");
      title.textContent = "首页";
      document.head.append(title);
    }
  });

  afterEach(() => {
    window.history.replaceState(null, "", "/");
    delete (window as { __TAURI_INTERNALS__?: object }).__TAURI_INTERNALS__;
    delete (window as { SzuApplicationBack?: unknown }).SzuApplicationBack;
  });

  it("syncs the native window title when the document title changes", async () => {
    (window as { __TAURI_INTERNALS__?: object }).__TAURI_INTERNALS__ = {};
    const setTitle = setTitleMock();

    const stop = startChildWindowTitleSync();
    document.title = "OJ · 编程练习";

    await vi.waitFor(() => expect(setTitle).toHaveBeenCalledWith("OJ · 编程练习"));
    stop();
    document.title = "设置";
    expect(setTitle).toHaveBeenCalledTimes(1);
  });

  it("does nothing outside the Tauri runtime", () => {
    const setTitle = setTitleMock();
    const stop = startChildWindowTitleSync();
    document.title = "学习";
    expect(setTitle).not.toHaveBeenCalled();
    stop();
  });

  it("does nothing when the Android native back bridge is present", () => {
    (window as { __TAURI_INTERNALS__?: object }).__TAURI_INTERNALS__ = {};
    (window as { SzuApplicationBack?: object }).SzuApplicationBack = {};
    const setTitle = setTitleMock();
    const stop = startChildWindowTitleSync();
    document.title = "学习";
    expect(setTitle).not.toHaveBeenCalled();
    stop();
  });
});
