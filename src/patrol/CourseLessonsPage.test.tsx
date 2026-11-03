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

describe("CourseLessonsPage", () => {
  const invokeMock = vi.mocked(invoke);

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it("渲染选集课次，开课中课次展示开课中红色Chip", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_auth_state") {
        return { Authenticated: { studentId: "2025150041", name: "测试" } };
      }
      if (command === "get_onboarding_state") {
        return { steps: [], needsSetup: false, currentStep: null };
      }
      if (command === "jxxk_list_lessons") {
        return {
          pageIndex: 1,
          pageSize: 200,
          recordCount: 2,
          content: [
            {
              id: "lesson-1",
              targetName: "专业英语第10周星期2第9-10节",
              liveId: "live-1",
              liveState: 2,
              liveStateText: "开课中",
              startTime: "2026-11-03 16:00:00",
              endTime: "2026-11-03 17:25:00",
              classroom: "致理楼L1-303",
              mediaVideoId: null,
            },
            {
              id: "lesson-2",
              targetName: "专业英语第9周星期2第9-10节",
              liveId: "live-2",
              liveState: 4,
              liveStateText: "可看回放",
              startTime: "2026-10-27 16:00:00",
              endTime: "2026-10-27 17:25:00",
              classroom: "致理楼L1-303",
              mediaVideoId: "media-2",
            },
          ],
        };
      }
      return null;
    });

    const history = createMemoryHistory({
      initialEntries: [
        "/study/patrol/course?courseId=course-eng&course=%E4%B8%93%E4%B8%9A%E8%8B%B1%E8%AF%AD&teacher=%E5%BC%A0%E8%80%81%E5%B8%88",
      ],
    });
    const router = createRouter({ routeTree, history });
    render(<RouterProvider router={router} />);

    await waitFor(() => {
      expect(screen.getAllByText("专业英语").length).toBeGreaterThan(0);
      expect(screen.getByText("共 2 个课次")).toBeInTheDocument();
    });

    expect(screen.getByText("开课中")).toBeInTheDocument();
    expect(screen.getByText("可看回放")).toBeInTheDocument();
  });

  it("后台重验证当前课程课次时刷新按钮处于锁定状态", async () => {
    invokeMock.mockImplementation(async (command: string, args0?: unknown) => {
      if (command === "get_auth_state") {
        return { Authenticated: { studentId: "2025150041", name: "测试" } };
      }
      if (command === "get_onboarding_state") {
        return { steps: [], needsSetup: false, currentStep: null };
      }
      if (command === "jxxk_list_lessons") {
        return {
          pageIndex: 1,
          pageSize: 200,
          recordCount: 1,
          content: [
            {
              id: "lesson-1",
              targetName: "专业英语第10周",
              liveState: 4,
              liveStateText: "可看回放",
            },
          ],
        };
      }
      if (command === "subscribe_jxxk_state") {
        const { channel } = args0 as { channel: { onmessage: (e: unknown) => void } };
        channel.onmessage?.({
          type: "initial",
          data: {
            snapshot: { session: "ready", message: "", semesters: [], refreshing: true },
            courses: {},
            lessons: {},
            revalidatingKeys: ["lessons:course-eng"],
          },
        });
        return null;
      }
      return null;
    });

    const history = createMemoryHistory({
      initialEntries: [
        "/study/patrol/course?courseId=course-eng&course=%E4%B8%93%E4%B8%9A%E8%8B%B1%E8%AF%AD",
      ],
    });
    const router = createRouter({ routeTree, history });
    render(<RouterProvider router={router} />);

    const refreshBtn = await screen.findByRole("button", { name: "刷新课次列表" });
    await waitFor(() => {
      expect(refreshBtn).toBeDisabled();
    });
  });
});
