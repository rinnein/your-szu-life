/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import AboutPage, { updateStatus } from "./AboutPage";
import { useAppContent } from "./app-content/context";
import * as osInfo from "./os-info";
import type { AppContentSnapshot } from "./app-content/types";

vi.mock("./app-content/context", () => ({
  useAppContent: vi.fn(),
}));

// 构建产物里可能带着贡献者快照；这里固定走运行时回退，避免本地构建后的生成物影响断言。
vi.mock("./app-content/contributors-static", () => ({ staticContributors: null }));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(null),
  Channel: class MockChannel {
    onmessage = null;
  },
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
    repository: {
      fullName: "rinnein/your-szu-life",
      url: "https://github.com/rinnein/your-szu-life",
    },
    repositoryMessage: null,
    refreshing: false,
    nextAutomaticCheckAt: null,
    updateChannel: "stable",
    updateChannelFallback: false,
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
            latest_ver_url:
              "https://github.com/rinnein/your-szu-life/releases/download/canary/your-szu-life_android_arm64_0.9.1-release.apk.gz",
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
    vi.spyOn(osInfo, "getPlatform").mockResolvedValue("unknown");
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

    const cachedWithNewVersion = makeSnapshot(true, "0.9.1", true);
    expect(updateStatus(cachedWithNewVersion, false)).toBe("发现新版本 0.9.1（缓存）");

    const cachedWithError = makeSnapshot(true, "0.9.1", true);
    cachedWithError.update.error = {
      kind: "rateLimited" as never,
      message: "GitHub 请求受限，请稍后再试。",
      retryAt: null,
    };
    expect(updateStatus(cachedWithError, false)).toBe("发现新版本 0.9.1（缓存）");

    const cachedUpToDateWithError = makeSnapshot(false, "0.9.0", true);
    cachedUpToDateWithError.update.error = {
      kind: "rateLimited" as never,
      message: "GitHub 请求受限，请稍后再试。",
      retryAt: null,
    };
    expect(updateStatus(cachedUpToDateWithError, false)).toBe("上次检查未发现新版本（缓存）");

    const errorWithoutData = makeSnapshot(false, undefined, false);
    errorWithoutData.update.error = {
      kind: "rateLimited" as never,
      message: "GitHub 请求受限，请稍后再试。",
      retryAt: null,
    };
    expect(updateStatus(errorWithoutData, false)).toBe("本次更新检查失败");
  });

  it("renders generic download button on generic desktop", async () => {
    vi.spyOn(osInfo, "getPlatform").mockResolvedValue("macos");
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
    vi.spyOn(osInfo, "getPlatform").mockResolvedValue("android");
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

  it("renders download and install button on Windows", async () => {
    vi.spyOn(osInfo, "getPlatform").mockResolvedValue("windows");
    const snapshot = makeSnapshot(true, "0.9.1");
    vi.mocked(useAppContent).mockReturnValue({
      snapshot,
      error: undefined,
      refreshing: false,
      refresh: vi.fn(),
    });

    render(<AboutPage />);

    expect(await screen.findByText(/下载并安装 0.9.1/i)).toBeInTheDocument();
    expect(screen.getByText(/前往浏览器下载/i)).toBeInTheDocument();
  });

  it("renders release page jump button on Linux", async () => {
    vi.spyOn(osInfo, "getPlatform").mockResolvedValue("linux");
    const snapshot = makeSnapshot(true, "0.9.1");
    vi.mocked(useAppContent).mockReturnValue({
      snapshot,
      error: undefined,
      refreshing: false,
      refresh: vi.fn(),
    });

    render(<AboutPage />);

    expect(await screen.findByText(/前往发布页面 0.9.1/i)).toBeInTheDocument();
  });

  it("renders CNB source indicator when source is cnb", async () => {
    const snapshot = makeSnapshot(true, "0.9.1");
    snapshot.update.source = "cnb";
    snapshot.update.lastSuccessAt = 1700000000000;
    snapshot.announcements.source = "cnb";
    snapshot.announcements.lastSuccessAt = 1700000000000;
    vi.mocked(useAppContent).mockReturnValue({
      snapshot,
      error: undefined,
      refreshing: false,
      refresh: vi.fn(),
    });

    render(<AboutPage />);

    const cnbBadges = await screen.findAllByText(/CNB 镜像/i);
    expect(cnbBadges.length).toBeGreaterThanOrEqual(1);
  });

  it("renders the changelog as markdown", async () => {
    const snapshot = makeSnapshot(true, "0.9.1");
    snapshot.update.data!.changelog = "**重要**变更\n\n- 第一项";
    vi.mocked(useAppContent).mockReturnValue({
      snapshot,
      error: undefined,
      refreshing: false,
      refresh: vi.fn(),
    });

    render(<AboutPage />);

    expect((await screen.findByText("重要")).tagName).toBe("STRONG");
    expect(screen.getByRole("listitem")).toHaveTextContent("第一项");
  });

  it("switches the update channel through the backend", async () => {
    // 未配置自定义内容源（status=unconfigured）且有仓库时，更新来自 Release，
    // 频道切换才有意义。
    const snapshot = makeSnapshot(false, "0.9.0");
    snapshot.configuration = { status: "unconfigured", message: null };
    vi.mocked(useAppContent).mockReturnValue({
      snapshot,
      error: undefined,
      refreshing: false,
      refresh: vi.fn(),
    });

    render(<AboutPage />);

    const combobox = await screen.findByRole("combobox", { name: "更新频道" });
    const host = (combobox.getRootNode() as ShadowRoot).host ?? combobox;
    await waitFor(() => expect(host).not.toHaveAttribute("disabled"));
    fireEvent(host, new CustomEvent("change", { detail: { value: "canary" } }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("set_update_channel", { channel: "canary" }),
    );
  });

  it("explains the canary fallback and disables the switch for custom feeds", async () => {
    const fallback = makeSnapshot(false, "0.9.0");
    fallback.configuration = { status: "unconfigured", message: null };
    fallback.updateChannelFallback = true;
    vi.mocked(useAppContent).mockReturnValue({
      snapshot: fallback,
      error: undefined,
      refreshing: false,
      refresh: vi.fn(),
    });
    const { unmount } = render(<AboutPage />);
    expect(
      await screen.findByText(/正式版暂无 Release，当前回退使用测试版构建/),
    ).toBeInTheDocument();
    unmount();

    // 编译期自定义内容源与频道无关：下拉必须禁用并给出说明。
    const custom = makeSnapshot(false, "0.9.0");
    vi.mocked(useAppContent).mockReturnValue({
      snapshot: custom,
      error: undefined,
      refreshing: false,
      refresh: vi.fn(),
    });
    render(<AboutPage />);
    const combobox = await screen.findByRole("combobox", { name: "更新频道" });
    const host = (combobox.getRootNode() as ShadowRoot).host ?? combobox;
    expect(host).toHaveAttribute("disabled");
    expect(screen.getByText(/当前构建使用自定义内容源，频道切换不生效/)).toBeInTheDocument();
  });
});
