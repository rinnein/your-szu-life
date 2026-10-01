/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { AnnouncementBoard, AnnouncementImage } from "./AnnouncementBoard";
import { useAppContent } from "./context";
import type { Announcement, AppContentSnapshot, ContentError } from "./types";
import { EMPTY_ELECTRICITY_STATUS, useElectricityMonitor } from "../electricity-monitor";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("./context", () => ({
  useAppContent: vi.fn(),
}));

vi.mock("../electricity-monitor", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../electricity-monitor")>()),
  useElectricityMonitor: vi.fn(),
}));

vi.mock("../logical-navigation", () => ({
  useLogicalNavigation: () => ({
    push: vi.fn(),
    replace: vi.fn(),
    back: vi.fn(),
    canBack: () => false,
  }),
}));

const announcements: Announcement[] = [
  { title: "公告一", pic: "https://example.com/one.png", desc: "第一条公告" },
  { title: "公告二", pic: "https://example.com/two.png", desc: "第二条公告" },
  { title: "公告三", pic: "https://example.com/three.png", goto: "https://example.com/three" },
];

function makeSnapshot(
  units: Announcement[] | undefined,
  feedError: ContentError | null = null,
): AppContentSnapshot {
  return {
    revision: 1,
    version: "0.1.0",
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
      error: feedError,
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

function mockContent(snapshot: AppContentSnapshot, error?: string) {
  vi.mocked(useAppContent).mockReturnValue({
    snapshot,
    error,
    refreshing: false,
    refresh: vi.fn().mockResolvedValue(undefined),
  });
}

describe("AnnouncementBoard carousel", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockResolvedValue("data:image/png;base64,AA==");
    vi.mocked(useElectricityMonitor).mockReturnValue(EMPTY_ELECTRICITY_STATUS);
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it.each([undefined, null, "", "  "])(
    "shows a static placeholder without requesting an absent picture (%s)",
    (pic) => {
      mockContent(makeSnapshot([{ title: "无图公告", pic, desc: "公告正文" }]));
      render(<AnnouncementBoard />);

      expect(screen.getByRole("img", { name: "无图公告暂无头图" })).toBeInTheDocument();
      expect(screen.queryByText("正在加载头图")).not.toBeInTheDocument();
      expect(invoke).not.toHaveBeenCalled();
      fireEvent.click(screen.getByRole("button", { name: /无图公告/ }));
      expect(screen.getByText("公告正文")).toBeInTheDocument();
    },
  );

  it("replaces a loaded picture with a placeholder when the next item has no picture", async () => {
    const { rerender } = render(
      <AnnouncementImage url="https://example.com/one.png" title="有图公告" />,
    );
    expect(await screen.findByRole("img", { name: "有图公告头图" })).toBeInTheDocument();

    rerender(<AnnouncementImage title="无图公告" />);

    expect(screen.getByRole("img", { name: "无图公告暂无头图" })).toBeInTheDocument();
    expect(screen.queryByRole("img", { name: "有图公告头图" })).not.toBeInTheDocument();
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("gives the local electricity notice the same image slot as a remote picture", async () => {
    vi.mocked(useElectricityMonitor).mockReturnValue({
      ...EMPTY_ELECTRICITY_STATUS,
      low: true,
      remaining: 5,
    });
    mockContent(makeSnapshot([announcements[0]]));
    render(<AnnouncementBoard />);

    const placeholder = screen.getByRole("img", { name: "当前剩余电量：5.00 度暂无头图" });
    expect(placeholder.parentElement).toHaveClass("announcement-image");
    expect(invoke).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "下一条公告" }));
    expect((await screen.findByRole("img", { name: "公告一头图" })).parentElement).toHaveClass(
      "announcement-image",
    );
  });

  it("renders one announcement at a time and switches with controls and indicators", () => {
    mockContent(makeSnapshot(announcements));
    render(<AnnouncementBoard />);

    expect(screen.getAllByTestId("announcement-slide")).toHaveLength(1);
    expect(screen.getByRole("group", { name: "第 1 条公告，共 3 条" })).toBeInTheDocument();
    expect(screen.getByText("公告一")).toBeInTheDocument();
    expect(screen.queryByText("公告二")).not.toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /查看第/ })).toHaveLength(3);
    expect(screen.getAllByRole("button", { name: /查看第/ })[0]).toHaveAttribute(
      "aria-current",
      "true",
    );

    fireEvent.click(screen.getByRole("button", { name: "下一条公告" }));
    expect(screen.getByRole("group", { name: "第 2 条公告，共 3 条" })).toBeInTheDocument();
    expect(screen.getByText("公告二")).toBeInTheDocument();
    expect(screen.queryByText("公告一")).not.toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /查看第/ })[1]).toHaveAttribute(
      "aria-current",
      "true",
    );

    fireEvent.click(screen.getByRole("button", { name: "上一条公告" }));
    expect(screen.getByRole("group", { name: "第 1 条公告，共 3 条" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "查看第 3 条公告" }));
    expect(screen.getByRole("group", { name: "第 3 条公告，共 3 条" })).toBeInTheDocument();
    expect(screen.getByText("公告三")).toBeInTheDocument();
  });

  it("autoplays and pauses while the carousel is hovered", () => {
    vi.useFakeTimers();
    mockContent(makeSnapshot(announcements));
    render(<AnnouncementBoard />);

    const carousel = screen.getByTestId("announcement-carousel");
    fireEvent.mouseEnter(carousel);
    act(() => {
      vi.advanceTimersByTime(5000);
    });
    expect(screen.getByRole("group", { name: "第 1 条公告，共 3 条" })).toBeInTheDocument();

    fireEvent.mouseLeave(carousel);
    act(() => {
      vi.advanceTimersByTime(5000);
    });
    expect(screen.getByRole("group", { name: "第 2 条公告，共 3 条" })).toBeInTheDocument();
    vi.useRealTimers();
  });

  it("hides carousel controls when there is only one announcement", () => {
    mockContent(makeSnapshot([announcements[0]]));
    render(<AnnouncementBoard />);

    expect(screen.getByRole("group", { name: "第 1 条公告，共 1 条" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "上一条公告" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "下一条公告" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /查看第/ })).not.toBeInTheDocument();
  });

  it("keeps the empty state when the announcement feed has no items", () => {
    mockContent(makeSnapshot([]));
    render(<AnnouncementBoard />);

    expect(screen.getByText("暂无公告")).toBeInTheDocument();
    expect(screen.queryByTestId("announcement-carousel")).not.toBeInTheDocument();
  });

  it("keeps the cached-feed warning when loading announcements fails", () => {
    mockContent(
      makeSnapshot([announcements[0]], {
        kind: "network",
        message: "网络暂不可用",
        retryAt: null,
      }),
    );
    render(<AnnouncementBoard />);

    expect(screen.getByText("本次公告获取失败")).toBeInTheDocument();
    expect(screen.getByText("网络暂不可用")).toBeInTheDocument();
    expect(screen.getByText("公告一")).toBeInTheDocument();
  });
});
