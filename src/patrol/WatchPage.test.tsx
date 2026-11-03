/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { createMemoryHistory, createRouter, RouterProvider } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";
import { routeTree } from "../routeTree.gen";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

// Mock ArtPlayer，避免在 jsdom 环境中缺少媒体上下文报错。
vi.mock("artplayer", () => {
  return {
    default: class MockArtplayer {
      public video = document.createElement("video");
      public events: Record<string, () => void> = {};
      constructor(options: { container: HTMLElement }) {
        options.container.appendChild(this.video);
      }
      on(event: string, fn: () => void) {
        this.events[event] = fn;
      }
      destroy() {
        this.video.remove();
      }
    },
  };
});

describe("WatchPage", () => {
  const invokeMock = vi.mocked(invoke);

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it("直播课次（liveState=2）即使 lessonStreams 报错也正常获取直播流并播放", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_auth_state") {
        return { Authenticated: { studentId: "2025150041", name: "测试" } };
      }
      if (cmd === "get_onboarding_state") {
        return { steps: [], needsSetup: false, currentStep: null };
      }
      if (cmd === "jxxk_live_stream") {
        return [
          { url: "https://jxxk.szu.edu.cn/live/teacher.m3u8", label: "教师画面" },
          { url: "https://jxxk.szu.edu.cn/live/pc.m3u8", label: "电脑画面" },
        ];
      }
      if (cmd === "jxxk_lesson_streams") {
        // 直播中平台尚未生成课堂录像，此接口报错是真实平台的常态行为
        throw new Error("该课次还没有可用的课堂录像");
      }
      if (cmd === "get_file_manager_state") {
        return { modules: [] };
      }
      return null;
    });

    const history = createMemoryHistory({
      initialEntries: [
        "/study/patrol/watch?courseId=course-1&liveId=live-1&course=%E6%95%B0%E6%8D%AE%E7%BB%93%E6%9E%84&liveState=2&title=%E7%AC%AC2%E5%91%A8%E7%9B%B4%E6%92%AD",
      ],
    });
    const router = createRouter({ routeTree, history });
    render(<RouterProvider router={router} />);

    // 标题与说明应展示为直播中
    await waitFor(() => {
      expect(screen.getByText(/第2周直播/)).toBeInTheDocument();
      expect(screen.getByText(/数据结构 · 直播中/)).toBeInTheDocument();
    });

    // 画面选择工具栏与画面角标应展示直播流名称
    await waitFor(() => {
      expect(screen.getAllByText("教师画面").length).toBeGreaterThan(0);
      expect(screen.getAllByText("电脑画面").length).toBeGreaterThan(0);
    });

    // 绝不能出现“该课次还没有可用的课堂录像”错误提示
    expect(screen.queryByText(/该课次还没有可用的课堂录像/)).not.toBeInTheDocument();
    expect(screen.queryByText("回放暂不可用")).not.toBeInTheDocument();
    expect(screen.queryByText("直播暂不可用")).not.toBeInTheDocument();
  });

  it("回放课次（liveState=4）正常获取录像与播放地址", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_auth_state") {
        return { Authenticated: { studentId: "2025150041", name: "测试" } };
      }
      if (cmd === "get_onboarding_state") {
        return { steps: [], needsSetup: false, currentStep: null };
      }
      if (cmd === "jxxk_lesson_streams") {
        return {
          videoId: "v1",
          mediaVideoId: "mv1",
          title: "第1周回放",
          duration: 3600,
          download: true,
          streams: [
            { id: "s1", name: "教师画面", streamNo: "1", fileSize: 1024000, duration: 3600 },
          ],
        };
      }
      if (cmd === "jxxk_play_urls") {
        return [
          {
            screenTag: "teacher",
            streamName: "教师画面",
            streamNo: "1",
            playUrl: "https://jxxk.szu.edu.cn/video/teacher.m3u8",
          },
        ];
      }
      if (cmd === "get_file_manager_state") {
        return { modules: [] };
      }
      return null;
    });

    const history = createMemoryHistory({
      initialEntries: [
        "/study/patrol/watch?courseId=course-1&liveId=live-1&course=%E6%95%B0%E6%8D%AE%E7%BB%93%E6%9E%84&liveState=4&title=%E7%AC%AC1%E5%91%A8%E5%9B%9E%E6%94%BE",
      ],
    });
    const router = createRouter({ routeTree, history });
    render(<RouterProvider router={router} />);

    await waitFor(() => {
      expect(screen.getByText(/第1周回放/)).toBeInTheDocument();
      expect(screen.getByText(/时长 1:00:00/)).toBeInTheDocument();
      expect(screen.getByText("下载录像")).toBeInTheDocument();
    });
  });

  it("直播课次在 jxxk_live_stream 为空时自动从 jxxk_play_urls 兜底获取流并播放", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_auth_state") {
        return { Authenticated: { studentId: "2025150041", name: "测试" } };
      }
      if (cmd === "get_onboarding_state") {
        return { steps: [], needsSetup: false, currentStep: null };
      }
      if (cmd === "jxxk_live_stream") {
        // 录播未配置 RTMP 直播流，返回空列表
        return [];
      }
      if (cmd === "jxxk_play_urls") {
        // 录播系统主机实时切片的 HLS 流
        return [
          {
            screenTag: "multi",
            streamName: "电脑画面",
            streamNo: "30",
            playUrl: "https://jxxk.szu.edu.cn/video/pc.m3u8",
          },
          {
            screenTag: "multi",
            streamName: "教师画面",
            streamNo: "1",
            playUrl: "https://jxxk.szu.edu.cn/video/teacher.m3u8",
          },
        ];
      }
      if (cmd === "get_file_manager_state") {
        return { modules: [] };
      }
      return null;
    });

    const history = createMemoryHistory({
      initialEntries: [
        "/study/patrol/watch?courseId=course-1&liveId=live-1&course=%E4%B8%93%E4%B8%9A%E8%8B%B1%E8%AF%AD&liveState=2&title=%E4%B8%93%E4%B8%9A%E8%8B%B1%E8%AF%AD%E7%AC%AC10%E5%91%A8%E7%9B%B4%E6%92%AD",
      ],
    });
    const router = createRouter({ routeTree, history });
    render(<RouterProvider router={router} />);

    // 成功从 play_urls 兜底获取画面并展示
    await waitFor(() => {
      expect(screen.getAllByText("电脑画面").length).toBeGreaterThan(0);
      expect(screen.getAllByText("教师画面").length).toBeGreaterThan(0);
    });

    expect(screen.queryByText(/当前暂未检测到可用直播信号/)).not.toBeInTheDocument();
    expect(screen.queryByText("直播暂不可用")).not.toBeInTheDocument();
  });
});
