/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import AboutPage, { updateStatus } from "./AboutPage";
import { useAppContent } from "./app-content/context";
import * as osInfo from "./os-info";
import type { AppContentSnapshot } from "./app-content/types";

vi.mock("./app-content/context", () => ({
  useAppContent: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(null),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock("./components", () => ({
  BrandMark: () => <div data-testid="brand-mark" />,
  Card: ({ title, children }: { title: string; children: React.ReactNode }) => (
    <section>
      <h2>{title}</h2>
      {children}
    </section>
  ),
  ModuleHeader: ({ title }: { title: string }) => <header>{title}</header>,
  PageShell: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));

function makeSnapshot(
  updateAvailable: boolean,
  latestVer?: string,
  fromCache = false,
): AppContentSnapshot {
  return {
    revision: 1,
    version: "0.9.0",
    configuration: { status: "ready", message: null },
    repository: { fullName: "rinnein/your-szu-life", url: "https://github.com/rinnein/your-szu-life" },
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
            latest_ver_url: "https://github.com/rinnein/your-szu-life/releases/download/canary/your-szu-life_android_arm64_0.9.1-release.apk.gz",
            changelog: "更新说明",
          }
        : null,
      lastAttemptAt: null,
      lastSuccessAt: null,
      source: "github",
      fromCache,
      error: null,
    },
    updateAvailable,
  };
}

describe("AboutPage update logic", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.spyOn(osInfo, "isAndroidPlatform").mockResolvedValue(false);
  });

  afterEach(() => {
    cleanup();
  });

  it("calculates correct updateStatus string", () => {
    expect(updateStatus(undefined, true)).toBe("正在检查公告和更新…");
    expect(updateStatus(undefined, false, "error")).toBe("本次检查未能完成");
    expect(updateStatus(undefined, false)).toBe("正在读取更新状态…");

    const snapshot = makeSnapshot(true, "0.9.1");
    expect(updateStatus(snapshot, false)).toBe("发现新版本 0.9.1");

    const upToDate = makeSnapshot(false, "0.9.0");
    expect(updateStatus(upToDate, false)).toBe("当前已是最新版本");

    const cachedUpToDate = makeSnapshot(false, "0.9.0", true);
    expect(updateStatus(cachedUpToDate, false)).toBe("上次检查未发现新版本（缓存）");
  });

  it("renders desktop download button on desktop", async () => {
    const snapshot = makeSnapshot(true, "0.9.1");
    vi.mocked(useAppContent).mockReturnValue({
      snapshot,
      error: undefined,
      refreshing: false,
      refresh: vi.fn(),
    });

    render(<AboutPage />);

    expect(await screen.findByText(/前往下载 0.9.1/i)).toBeInTheDocument();
  });

  it("renders in-app update button on Android", async () => {
    vi.spyOn(osInfo, "isAndroidPlatform").mockResolvedValue(true);
    const snapshot = makeSnapshot(true, "0.9.1");
    vi.mocked(useAppContent).mockReturnValue({
      snapshot,
      error: undefined,
      refreshing: false,
      refresh: vi.fn(),
    });

    render(<AboutPage />);

    expect(await screen.findByText(/应用内更新 0.9.1/i)).toBeInTheDocument();
    expect(screen.getByText(/前往浏览器下载/i)).toBeInTheDocument();
  });
});
