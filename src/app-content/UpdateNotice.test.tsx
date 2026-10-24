/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { UpdateNotice } from "./UpdateNotice";
import { useAppContent } from "./context";
import { useRouterState } from "@tanstack/react-router";
import * as osInfo from "../os-info";
import * as external from "./external";
import * as postponeModule from "./update-postpone";
import type { AppContentSnapshot } from "./types";

vi.mock("./context", () => ({
  useAppContent: vi.fn(),
}));

vi.mock("@tanstack/react-router", () => ({
  useRouterState: vi.fn(),
}));

const mockInvoke = vi.fn().mockResolvedValue(null);
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
  Channel: class MockChannel {
    onmessage = null;
  },
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

function makeSnapshot(updateAvailable: boolean, latestVer?: string): AppContentSnapshot {
  return {
    revision: 1,
    version: "0.1.0",
    configuration: { status: "ready", message: null },
    repository: null,
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
            latest_ver_url: "https://example.com/download.msi",
            release_url: "https://github.com/rinnein/your-szu-life/releases/tag/v0.2.0",
            changelog: "修复已知问题",
          }
        : null,
      lastAttemptAt: null,
      lastSuccessAt: null,
      source: "primary",
      fromCache: false,
      error: null,
    },
    updateAvailable,
  };
}

describe("UpdateNotice dialog", () => {
  let currentPath = "/";

  beforeEach(() => {
    vi.restoreAllMocks();
    localStorage.clear();
    currentPath = "/";
    vi.mocked(useRouterState).mockImplementation(((opts?: {
      select?: (state: unknown) => unknown;
    }) => {
      const state = { location: { pathname: currentPath } };
      return opts?.select ? opts.select(state) : state;
    }) as never);
    vi.spyOn(osInfo, "getPlatform").mockResolvedValue("windows");
    vi.spyOn(external, "openPublicUrl").mockResolvedValue(undefined);
  });

  afterEach(() => {
    cleanup();
    localStorage.clear();
  });

  it("renders update dialog when update is available and not on about page", async () => {
    vi.mocked(useAppContent).mockReturnValue({
      snapshot: makeSnapshot(true, "0.2.0"),
      refreshing: false,
      refresh: vi.fn(),
    });

    render(<UpdateNotice />);

    expect(await screen.findByText("发现新版本 0.2.0")).toBeInTheDocument();
    expect(screen.getByText("稍后再说")).toBeInTheDocument();
    expect(screen.getByText("下载并安装")).toBeInTheDocument();
  });

  it("does not render dialog when on about page", () => {
    currentPath = "/about";
    vi.mocked(useAppContent).mockReturnValue({
      snapshot: makeSnapshot(true, "0.2.0"),
      refreshing: false,
      refresh: vi.fn(),
    });

    render(<UpdateNotice />);

    expect(screen.queryByText("发现新版本 0.2.0")).not.toBeInTheDocument();
  });

  it("does not render dialog when update is postponed", () => {
    vi.spyOn(postponeModule, "isUpdatePostponed").mockReturnValue(true);
    vi.mocked(useAppContent).mockReturnValue({
      snapshot: makeSnapshot(true, "0.2.0"),
      refreshing: false,
      refresh: vi.fn(),
    });

    render(<UpdateNotice />);

    expect(screen.queryByText("发现新版本 0.2.0")).not.toBeInTheDocument();
  });

  it("clicking '稍后再说' postpones update reminder and closes dialog", async () => {
    const postponeSpy = vi.spyOn(postponeModule, "postponeUpdateReminder");
    vi.mocked(useAppContent).mockReturnValue({
      snapshot: makeSnapshot(true, "0.2.0"),
      refreshing: false,
      refresh: vi.fn(),
    });

    render(<UpdateNotice />);

    const postponeBtn = await screen.findByRole("button", { name: "稍后再说" });
    fireEvent.click(postponeBtn);

    expect(postponeSpy).toHaveBeenCalled();
  });

  it("Windows platform: clicking '下载并安装' invokes start_in_app_update", async () => {
    vi.spyOn(osInfo, "getPlatform").mockResolvedValue("windows");
    vi.mocked(useAppContent).mockReturnValue({
      snapshot: makeSnapshot(true, "0.2.0"),
      refreshing: false,
      refresh: vi.fn(),
    });

    render(<UpdateNotice />);

    const updateBtn = await screen.findByRole("button", { name: "下载并安装" });
    fireEvent.click(updateBtn);

    expect(mockInvoke).toHaveBeenCalledWith("start_in_app_update", {
      onProgress: expect.any(Object),
    });
  });

  it("Android platform: renders in-app update and browser download buttons", async () => {
    vi.spyOn(osInfo, "getPlatform").mockResolvedValue("android");
    vi.mocked(useAppContent).mockReturnValue({
      snapshot: makeSnapshot(true, "0.2.0"),
      refreshing: false,
      refresh: vi.fn(),
    });

    render(<UpdateNotice />);

    expect(await screen.findByRole("button", { name: "应用内更新" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "浏览器下载" })).toBeInTheDocument();
  });

  it("Linux platform: clicking '前往发布页面' opens release url and postpones", async () => {
    vi.spyOn(osInfo, "getPlatform").mockResolvedValue("linux");
    const openSpy = vi.spyOn(external, "openPublicUrl");
    const postponeSpy = vi.spyOn(postponeModule, "postponeUpdateReminder");

    vi.mocked(useAppContent).mockReturnValue({
      snapshot: makeSnapshot(true, "0.2.0"),
      refreshing: false,
      refresh: vi.fn(),
    });

    render(<UpdateNotice />);

    const releaseBtn = await screen.findByRole("button", { name: "前往发布页面" });
    fireEvent.click(releaseBtn);

    expect(openSpy).toHaveBeenCalledWith(
      "https://github.com/rinnein/your-szu-life/releases/tag/v0.2.0",
    );
    expect(postponeSpy).toHaveBeenCalled();
  });
});
