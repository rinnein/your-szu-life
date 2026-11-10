/** @vitest-environment jsdom */

import { beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { trackEvent } from "@aptabase/tauri";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@aptabase/tauri", () => ({ trackEvent: vi.fn() }));

async function loadAnalytics() {
  return import("./analytics");
}

describe("analytics wrapper", () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  });

  it("does not touch Tauri or Aptabase outside the native runtime", async () => {
    const analytics = await loadAnalytics();
    analytics.trackAnalytics("screen_view", { route: "/" });
    await Promise.resolve();

    expect(invoke).not.toHaveBeenCalled();
    expect(trackEvent).not.toHaveBeenCalled();
  });

  it("sends events when the persisted setting is enabled", async () => {
    (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    vi.mocked(invoke).mockResolvedValue({ enabled: true });

    const analytics = await loadAnalytics();
    await analytics.initializeAnalytics();
    analytics.trackAnalytics("screen_view", { route: "/study/uooc" });

    await vi.waitFor(() => {
      expect(trackEvent).toHaveBeenCalledWith("screen_view", { route: "/study/uooc" });
    });
    expect(invoke).toHaveBeenCalledWith("get_analytics_settings");
  });

  it("stops sending after the user disables analytics", async () => {
    (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    vi.mocked(invoke).mockResolvedValue({ enabled: true });

    const analytics = await loadAnalytics();
    await analytics.initializeAnalytics();
    analytics.setAnalyticsEnabled(false);
    analytics.trackAnalytics("screen_view", { route: "/life" });

    await Promise.resolve();
    await Promise.resolve();
    expect(trackEvent).not.toHaveBeenCalled();
  });

  it("fails closed when the setting cannot be read", async () => {
    (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    vi.mocked(invoke).mockRejectedValue(new Error("settings unavailable"));

    const analytics = await loadAnalytics();
    await analytics.initializeAnalytics();
    analytics.trackAnalytics("screen_view", { route: "/settings" });

    await Promise.resolve();
    await Promise.resolve();
    expect(trackEvent).not.toHaveBeenCalled();
  });
});
