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

// 「今天」的日期串与星期：无参数用例依赖「默认过滤到今天」。
const pad = (n: number) => String(n).padStart(2, "0");
const now = new Date();
const todayStr = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
const todayWeekdayNum = now.getDay() === 0 ? 7 : now.getDay();

const semesters = [
  { id: "sem-1", name: "2026-2027-1", beginDate: "2026-08-31", endDate: "2027-01-17" },
];

const courses = [
  {
    id: "course-a",
    name: "数据结构",
    teacher: "张老师",
    yearSemesterId: "sem-1",
    yearSemesterName: "2026-2027-1",
  },
  {
    id: "course-b",
    name: "高等数学",
    teacher: "李老师",
    yearSemesterId: "sem-1",
    yearSemesterName: "2026-2027-1",
  },
];

// 数据结构：2026-09-04 是星期五；高等数学：2026-09-07 是星期一。
const lessonsByCourse: Record<string, unknown> = {
  "course-a": {
    pageIndex: 1,
    pageSize: 200,
    recordCount: 2,
    content: [
      {
        id: "lesson-a1",
        targetName: "数据结构第1周星期5第3-5节",
        liveId: "live-a1",
        liveState: 4,
        liveStateText: "查看回放",
        startTime: "2026-09-04 10:15:00",
        endTime: "2026-09-04 12:25:00",
        classroom: "致理楼L1-703",
        mediaVideoId: "media-a1",
      },
      {
        id: "lesson-a2",
        targetName: "数据结构第2周星期5第3-5节",
        liveId: "live-a2",
        liveState: 2,
        liveStateText: "直播中",
        startTime: "2026-09-11 10:15:00",
        endTime: "2026-09-11 12:25:00",
        classroom: "致理楼L1-703",
        mediaVideoId: null,
      },
      {
        id: "lesson-a3",
        targetName: `数据结构第9周星期${todayWeekdayNum === 7 ? "日" : todayWeekdayNum}第6-9节`,
        liveId: "live-a3",
        liveState: 2,
        liveStateText: "直播中",
        startTime: `${todayStr} 13:30:00`,
        endTime: `${todayStr} 16:40:00`,
        classroom: "南座-310",
        mediaVideoId: null,
      },
    ],
  },
  "course-b": {
    pageIndex: 1,
    pageSize: 200,
    recordCount: 1,
    content: [
      {
        id: "lesson-b1",
        targetName: "高等数学第1周星期1第1-2节",
        liveId: "live-b1",
        liveState: 1,
        liveStateText: "未开始",
        startTime: "2026-09-07 08:00:00",
        endTime: "2026-09-07 09:40:00",
        classroom: "科技楼",
        mediaVideoId: null,
      },
    ],
  },
};

function setupInvoke(session: string) {
  vi.mocked(invoke).mockImplementation(async (command: string, args0?: unknown) => {
    if (command === "get_auth_state") {
      return session === "authenticated"
        ? { Authenticated: { studentId: "2025150041", name: "测试" } }
        : { [session]: null };
    }
    if (command === "get_jxxk_snapshot" || command === "refresh_jxxk") {
      return { session: "ready", message: "", semesters, fetchedAt: 1 };
    }
    if (command === "jxxk_list_courses") {
      return { pageIndex: 1, pageSize: 200, recordCount: courses.length, content: courses };
    }
    if (command === "get_onboarding_state") {
      return { steps: [], needsSetup: false, currentStep: null };
    }
    if (command === "jxxk_list_lessons") {
      const args = args0 as { courseId: string };
      return lessonsByCourse[args.courseId];
    }
    return null;
  });
}

async function renderPatrol(initialPath = "/study/patrol") {
  const history = createMemoryHistory({ initialEntries: [initialPath] });
  const router = createRouter({ routeTree, history });
  render(<RouterProvider router={router} />);
  return router;
}

beforeEach(() => {
  setupInvoke("authenticated");
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("PatrolPage", () => {
  it("无参数进入时把默认学期回写到 URL", async () => {
    const router = await renderPatrol("/study/patrol");
    await screen.findByText("数据结构");
    await waitFor(() => {
      expect(router.state.location.search.semester).toBe("sem-1");
    });
  });

  it("无效学期参数同样回写为当前学期", async () => {
    const router = await renderPatrol("/study/patrol?semester=bogus");
    await screen.findByText("数据结构");
    await waitFor(() => {
      expect(router.state.location.search.semester).toBe("sem-1");
    });
  });

  it("按星期聚合为课程卡片，直播中课程置顶", async () => {
    renderPatrol("/study/patrol?weekday=5");
    // 课程卡片：数据结构（星期五 2 个课次，其中一个直播中）。
    expect(await screen.findByText("数据结构")).toBeInTheDocument();
    expect(screen.getByText("2 个可看课次")).toBeInTheDocument();
    // 直播中徽标出现在课程卡片上。
    expect(screen.getByText("直播中")).toBeInTheDocument();
    // 星期五之外（星期一）的高等数学不出现在默认列表。
    expect(screen.queryByText("高等数学")).not.toBeInTheDocument();
  });

  it("weekday=all 显示全部星期的课程", async () => {
    renderPatrol("/study/patrol?weekday=all");
    expect(await screen.findByText("数据结构")).toBeInTheDocument();
    // 高等数学唯一的课次是未开始（liveState=1）：未开始的课次不展示，
    // 没有可看课次的课程整门隐藏。
    expect(screen.queryByText("高等数学")).not.toBeInTheDocument();
  });

  it("会话未建立时显示引导 Banner", async () => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === "get_auth_state") {
        return { Authenticated: { studentId: "2025150041", name: "测试" } };
      }
      if (command === "get_onboarding_state") {
        return { steps: [], needsSetup: false, currentStep: null };
      }
      if (command === "get_jxxk_snapshot" || command === "refresh_jxxk") {
        return {
          session: "needsAuth",
          message: "请先完成应用统一认证，再使用监察巡课",
          semesters: [],
          fetchedAt: null,
        };
      }
      return null;
    });
    renderPatrol();
    expect(await screen.findByText("尚未获取到巡课会话")).toBeInTheDocument();
    expect(screen.getByText(/请先完成应用统一认证/)).toBeInTheDocument();
  });

  it("命中开课时间段乐观推进为开课中时，展示开课中标签", async () => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === "get_auth_state") {
        return { Authenticated: { studentId: "2025150041", name: "测试" } };
      }
      if (command === "get_onboarding_state") {
        return { steps: [], needsSetup: false, currentStep: null };
      }
      if (command === "get_jxxk_snapshot" || command === "refresh_jxxk") {
        return { session: "ready", message: "", semesters, fetchedAt: 1 };
      }
      if (command === "jxxk_list_courses") {
        return { pageIndex: 1, pageSize: 200, recordCount: courses.length, content: courses };
      }
      if (command === "jxxk_list_lessons") {
        return {
          pageIndex: 1,
          pageSize: 200,
          recordCount: 1,
          content: [
            {
              id: "lesson-in-session",
              targetName: "数据结构第1周星期5第3-5节",
              liveId: "live-in-session",
              liveState: 2,
              liveStateText: "开课中",
              startTime: "2026-09-04 10:15:00",
              endTime: "2026-09-04 12:25:00",
              classroom: "致理楼",
              mediaVideoId: null,
            },
          ],
        };
      }
      return null;
    });

    renderPatrol("/study/patrol?weekday=5");
    expect(await screen.findByText("数据结构")).toBeInTheDocument();
    expect(screen.getAllByText("开课中").length).toBeGreaterThan(0);
  });

  it("后台处于刷新状态时刷新按钮处于禁用状态", async () => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === "get_auth_state") {
        return { Authenticated: { studentId: "2025150041", name: "测试" } };
      }
      if (command === "get_onboarding_state") {
        return { steps: [], needsSetup: false, currentStep: null };
      }
      if (command === "get_jxxk_snapshot") {
        return { session: "ready", message: "", semesters, fetchedAt: 1, refreshing: true };
      }
      if (command === "jxxk_list_courses") {
        return { pageIndex: 1, pageSize: 200, recordCount: courses.length, content: courses };
      }
      if (command === "jxxk_list_lessons") {
        return { pageIndex: 1, pageSize: 200, recordCount: 0, content: [] };
      }
      return null;
    });

    renderPatrol("/study/patrol?weekday=all");
    const refreshBtn = await screen.findByRole("button", { name: "刷新巡课数据" });
    expect(refreshBtn).toBeDisabled();
  });
});
