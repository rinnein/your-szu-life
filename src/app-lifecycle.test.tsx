/** @vitest-environment jsdom */
import { cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { AppLifecycleBridge } from "./app-lifecycle";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
vi.mock("./logical-navigation", () => ({ isChildDocument: () => false }));

function setVisibility(state: "visible" | "hidden") {
  Object.defineProperty(document, "visibilityState", {
    configurable: true,
    get: () => state,
  });
}

describe("AppLifecycleBridge", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (window as any).__TAURI_INTERNALS__ = {};
    setVisibility("visible");
    vi.spyOn(document, "hasFocus").mockReturnValue(true);
  });

  afterEach(() => {
    cleanup();
    delete (window as any).__TAURI_INTERNALS__;
  });

  it("reports focus and visibility on mount and on window events", () => {
    render(<AppLifecycleBridge />);

    expect(invoke).toHaveBeenCalledWith("set_application_focus", { focused: true });
    expect(invoke).toHaveBeenCalledWith("set_application_visibility", { visible: true });

    vi.mocked(invoke).mockClear();
    vi.spyOn(document, "hasFocus").mockReturnValue(false);
    window.dispatchEvent(new Event("blur"));

    expect(invoke).toHaveBeenCalledWith("set_application_focus", { focused: false });
    expect(invoke).toHaveBeenCalledWith("set_application_visibility", { visible: true });
  });

  it("refreshes OJ monitoring when the app becomes visible again", () => {
    render(<AppLifecycleBridge />);
    vi.mocked(invoke).mockClear();

    setVisibility("hidden");
    document.dispatchEvent(new Event("visibilitychange"));
    expect(invoke).not.toHaveBeenCalledWith("refresh_oj");

    setVisibility("visible");
    document.dispatchEvent(new Event("visibilitychange"));
    expect(invoke).toHaveBeenCalledWith("refresh_oj");
  });
});
