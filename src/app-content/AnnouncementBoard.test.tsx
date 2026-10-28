/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { AnnouncementBoard, AnnouncementImage } from "./AnnouncementBoard";
import { useAppContent } from "./context";
import type { Announcement, AppContentSnapshot, ContentError } from "./types";
import * as externalModule from "./external";

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
  useLogicalNavigation: () => nav,
}));

const announcements: Announcement[] = [
  { title: "公告一", pic: "https://example.com/one.png", desc: "第一条公告内容描述" },
  { title: "公告二", pic: "https://example.com/two.png", desc: "第二条公告内容描述" },
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

describe("AnnouncementBoard compact bar & modal", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockResolvedValue("data:image/png;base64,AA==");
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it.each([undefined, null, "", "  "])(
    "shows a static placeholder without requesting an absent picture (%s)",
    (pic) => {
      render(<AnnouncementImage url={pic} title="无图公告" />);

      expect(screen.getByRole("img", { name: "无图公告暂无头图" })).toBeInTheDocument();
      expect(screen.queryByText("正在加载头图")).not.toBeInTheDocument();
      expect(invoke).not.toHaveBeenCalled();
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

  it("renders compact announcement bar with title and counter", () => {
    mockContent(makeSnapshot(announcements));
    render(<AnnouncementBoard />);

    const bar = screen.getByTestId("announcement-bar");
    expect(bar).toBeInTheDocument();
    expect(screen.getByText("公告一")).toBeInTheDocument();
    expect(screen.getByText("1/3")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "上一条公告" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "下一条公告" })).toBeInTheDocument();
  });

  it("cycles to next announcement when clicking next and prev", () => {
    mockContent(makeSnapshot(announcements));
    render(<AnnouncementBoard />);

    expect(screen.getByText("公告一")).toBeInTheDocument();
    expect(screen.getByText("1/3")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "下一条公告" }));
    expect(screen.getByText("公告二")).toBeInTheDocument();
    expect(screen.getByText("2/3")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "上一条公告" }));
    expect(screen.getByText("公告一")).toBeInTheDocument();
    expect(screen.getByText("1/3")).toBeInTheDocument();
  });

  it("opens modal dialog showing details on click and closes on dismiss", () => {
    mockContent(makeSnapshot(announcements));
    render(<AnnouncementBoard />);

    const barTrigger = screen.getByRole("button", { name: /查看公告详情：公告一/ });
    fireEvent.click(barTrigger);

    // Modal is opened
    expect(screen.getByRole("dialog", { name: "公告一" })).toBeInTheDocument();
    expect(screen.getByText("第一条公告内容描述")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "关闭" })).toBeInTheDocument();

    // Close modal
    fireEvent.click(screen.getByRole("button", { name: "关闭" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("opens in browser when goto URL is HTTP/HTTPS", () => {
    const openSpy = vi.spyOn(externalModule, "openPublicUrl").mockResolvedValue();
    mockContent(makeSnapshot([announcements[2]])); // Announcement 3 has external goto URL
    render(<AnnouncementBoard />);

    fireEvent.click(screen.getByRole("button", { name: /查看公告详情：公告三/ }));

    expect(screen.getByRole("dialog", { name: "公告三" })).toBeInTheDocument();
    const jumpBtn = screen.getByRole("button", { name: /在浏览器打开/ });
    expect(jumpBtn).toBeInTheDocument();

    fireEvent.click(jumpBtn);
    expect(openSpy).toHaveBeenCalledWith("https://example.com/three");
  });

  it("opens inside the app when goto URL is an internal route or deep link", () => {
    const internalAnnouncement: Announcement = {
      title: "选课提醒公告",
      desc: "选课系统已开放",
      goto: "/study/enrollment",
    };
    mockContent(makeSnapshot([internalAnnouncement]));
    render(<AnnouncementBoard />);

    fireEvent.click(screen.getByRole("button", { name: /查看公告详情：选课提醒公告/ }));

    expect(screen.getByRole("dialog", { name: "选课提醒公告" })).toBeInTheDocument();
    const internalBtn = screen.getByRole("button", { name: /在软件内打开/ });
    expect(internalBtn).toBeInTheDocument();

    fireEvent.click(internalBtn);
    expect(nav.push).toHaveBeenCalledWith("/study/enrollment");
    // Modal should be closed upon internal navigation
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("handles deep link URLs as internal app navigation", () => {
    const deepLinkAnnouncement: Announcement = {
      title: "OJ测验提醒",
      goto: "your-szu-life://page/study/oj?site=1",
    };
    mockContent(makeSnapshot([deepLinkAnnouncement]));
    render(<AnnouncementBoard />);

    fireEvent.click(screen.getByRole("button", { name: /查看公告详情：OJ测验提醒/ }));

    const internalBtn = screen.getByRole("button", { name: /在软件内打开/ });
    expect(internalBtn).toBeInTheDocument();

    fireEvent.click(internalBtn);
    expect(nav.push).toHaveBeenCalledWith("/study/oj?site=1");
  });

  it("pauses auto-cycling on hover and resumes on mouse leave", () => {
    vi.useFakeTimers();
    mockContent(makeSnapshot(announcements));
    render(<AnnouncementBoard />);

    const bar = screen.getByTestId("announcement-bar");
    fireEvent.mouseEnter(bar);

    act(() => {
      vi.advanceTimersByTime(5000);
    });
    // Still on first
    expect(screen.getByText("公告一")).toBeInTheDocument();

    fireEvent.mouseLeave(bar);
    act(() => {
      vi.advanceTimersByTime(5000);
    });
    // Advanced to second
    expect(screen.getByText("公告二")).toBeInTheDocument();
    vi.useRealTimers();
  });

  it("hides navigation controls when there is only one announcement", () => {
    mockContent(makeSnapshot([announcements[0]]));
    render(<AnnouncementBoard />);

    expect(screen.getByText("公告一")).toBeInTheDocument();
    expect(screen.queryByText("1/1")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "上一条公告" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "下一条公告" })).not.toBeInTheDocument();
  });

  it("renders empty bar when announcement feed has no items", () => {
    mockContent(makeSnapshot([]));
    render(<AnnouncementBoard />);

    expect(screen.queryByTestId("announcement-bar")).not.toBeInTheDocument();
    expect(screen.getByText("暂无公告")).toBeInTheDocument();
  });

  it("displays warning banner when feed fails to load", () => {
    mockContent(
      makeSnapshot([announcements[0]], {
        kind: "network",
        message: "网络暂不可用",
        retryAt: null,
      }),
    );
    render(<AnnouncementBoard />);

    expect(screen.getByText("本次公告获取失败")).toBeInTheDocument();
    expect(screen.getByText(/网络暂不可用/)).toBeInTheDocument();
  });
});
