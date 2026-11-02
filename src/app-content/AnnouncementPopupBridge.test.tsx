/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import {
  AnnouncementPopupBridge,
  resetAnnouncementAlertsForTesting,
} from "./AnnouncementPopupBridge";
import { markAnnouncementPopupSeen } from "./announcement-display";
import { useAppContent } from "./context";
import type { Announcement, AppContentSnapshot } from "./types";

const { nav } = vi.hoisted(() => ({
  nav: {
    push: vi.fn(),
    replace: vi.fn(),
    back: vi.fn(),
    canBack: () => false,
  },
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("./context", () => ({
  useAppContent: vi.fn(),
}));

vi.mock("../logical-navigation", () => ({
  isChildDocument: vi.fn(() => false),
  useLogicalNavigation: () => nav,
}));

function makeSnapshot(units: Announcement[] | undefined): AppContentSnapshot {
  return {
    revision: 1,
    version: "0.11.0",
    configuration: { status: "ready", message: null },
    repository: null,
    repositoryMessage: null,
    refreshing: false,
    nextAutomaticCheckAt: null,
    announcements: {
      data: units === undefined ? null : { announcements: units },
      lastAttemptAt: null,
      lastSuccessAt: units ? Date.now() : null,
      source: units ? "primary" : null,
      fromCache: false,
      error: null,
    },
    update: {
      data: null,
      lastAttemptAt: null,
      lastSuccessAt: null,
      source: null,
      fromCache: false,
      error: null,
    },
    updateAvailable: false,
  };
}

function mockContent(units: Announcement[] | undefined) {
  vi.mocked(useAppContent).mockReturnValue({
    snapshot: makeSnapshot(units),
    refreshing: false,
    refresh: vi.fn().mockResolvedValue(undefined),
  });
}

describe("AnnouncementPopupBridge global popups", () => {
  beforeEach(() => {
    localStorage.clear();
    resetAnnouncementAlertsForTesting();
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("shows nothing for legacy snapshots without mode fields", () => {
    mockContent([{ title: "普通公告", desc: "内容" }]);
    render(<AnnouncementPopupBridge />);

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("shows a popup once per id and records it in localStorage", () => {
    const popup: Announcement = {
      title: "定向公告",
      id: "v0.12-migration",
      mode: "popup",
      desc: "内容",
    };
    mockContent([popup]);
    const { rerender } = render(<AnnouncementPopupBridge />);

    expect(screen.getByRole("dialog", { name: "定向公告" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "关闭" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(isAnnouncementPopupSeenInStorage("v0.12-migration")).toBe(true);

    // 新快照到达（刷新）后同 id 不再弹出
    mockContent([popup]);
    rerender(<AnnouncementPopupBridge />);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("never shows a popup whose id was already seen", () => {
    markAnnouncementPopupSeen("already-seen");
    mockContent([{ title: "已看过的公告", id: "already-seen", mode: "popup", desc: "内容" }]);
    render(<AnnouncementPopupBridge />);

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("shows an alert once per session without touching localStorage", () => {
    const alert: Announcement = { title: "紧急公告", mode: "alert", desc: "内容" };
    mockContent([alert]);
    const { rerender } = render(<AnnouncementPopupBridge />);

    expect(screen.getByRole("dialog", { name: "紧急公告" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "关闭" }));
    mockContent([alert]);
    rerender(<AnnouncementPopupBridge />);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(localStorage.getItem("announcement_popup_seen_ids")).toBeNull();
  });

  it("queues multiple popups and alerts in announcement order", () => {
    mockContent([
      { title: "第一条", mode: "alert", desc: "一" },
      { title: "第二条", id: "second", mode: "popup", desc: "二" },
      { title: "第三条", mode: "alert", desc: "三" },
    ]);
    render(<AnnouncementPopupBridge />);

    expect(screen.getByRole("dialog", { name: "第一条" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "关闭" }));

    expect(screen.getByRole("dialog", { name: "第二条" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "关闭" }));

    expect(screen.getByRole("dialog", { name: "第三条" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "关闭" }));

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("ignores popups without an id (defensive, backend already degrades them)", () => {
    mockContent([{ title: "缺id公告", mode: "popup", desc: "内容" }]);
    render(<AnnouncementPopupBridge />);

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("shows nothing when the announcement feed is empty or not ready", () => {
    for (const units of [[], undefined]) {
      mockContent(units);
      const { unmount } = render(<AnnouncementPopupBridge />);
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      unmount();
    }
  });
});

function isAnnouncementPopupSeenInStorage(id: string): boolean {
  const raw = localStorage.getItem("announcement_popup_seen_ids");
  return raw ? (JSON.parse(raw) as string[]).includes(id) : false;
}
