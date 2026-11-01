/** @vitest-environment jsdom */

import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useHomeDeadlines } from "./use-home-deadlines";
import * as electricityModule from "../electricity-monitor";
import { ojProgressCache } from "../oj/progress-store";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

type Listener = (event: { payload: unknown }) => void;
const listeners = new Map<string, Listener>();

async function emit(event: string, payload?: unknown) {
  await act(async () => {
    listeners.get(event)?.({ payload });
  });
}

describe("useHomeDeadlines", () => {
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  beforeEach(() => {
    listeners.clear();
    vi.mocked(listen).mockImplementation((async (event: string, handler: Listener) => {
      listeners.set(event, handler);
      return () => listeners.delete(event);
    }) as unknown as typeof listen);
    vi.spyOn(ojProgressCache, "ensure").mockResolvedValue({ accepted: 1, total: 3 });
  });

  it("aggregates active OJ contest deadline and electricity outage forecast", async () => {
    const now = new Date("2026-10-28T10:00:00Z").getTime();
    vi.spyOn(Date, "now").mockReturnValue(now);

    // Mock electricity
    vi.spyOn(electricityModule, "useElectricityMonitor").mockReturnValue({
      remaining: 8.5,
      observedDate: "2026-10-28",
      expiresAtMs: now + 3600_000,
      expired: false,
      low: false,
      estimatedDaysLeft: 3.5, // outage in 3.5 days -> 2026-10-31
    });

    // Mock OJ and UOOC commands
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_oj_snapshot") {
        return {
          sites: [
            {
              id: "site1",
              name: "实验OJ",
              enabled: true,
              session: "ready",
              url: "http://oj",
              username: "u1",
            },
          ],
        };
      }
      if (command === "get_oj_groups") {
        return [{ id: "g1", name: "算法班" }];
      }
      if (command === "get_oj_contests") {
        return [
          {
            id: "c1",
            groupId: "g1",
            title: "算法测验 1",
            begin: now - 3600_000,
            end: now + 86400_000 * 2, // 2 days later -> 2026-10-30
            now,
            isExam: false,
          },
        ];
      }
      if (command === "get_uooc_snapshot") {
        return { session: "loginRequired", courses: [] };
      }
      return undefined;
    });

    const { result } = renderHook(() => useHomeDeadlines());

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.items).toHaveLength(2);

    const ojItem = result.current.items.find((item) => item.category === "oj-quiz");
    expect(ojItem).toBeDefined();
    expect(ojItem?.title).toBe("算法测验 1");
    expect(ojItem?.actionUrl).toContain("/study/oj?site=site1&contest=c1");

    const elecItem = result.current.items.find((item) => item.category === "electricity-outage");
    expect(elecItem).toBeDefined();
    expect(elecItem?.title).toContain("预计电量耗尽");
    expect(elecItem?.actionUrl).toBe("/life/campus-payments/recharge");

    expect(result.current.categoryMap.get(ojItem!.dateKey)).toContain("oj-quiz");
    expect(result.current.categoryMap.get(elecItem!.dateKey)).toContain("electricity-outage");
  });

  it("aggregates UOOC course end date, exam deadline, and homework deadline", async () => {
    const now = new Date("2026-10-28T10:00:00Z").getTime();
    vi.spyOn(Date, "now").mockReturnValue(now);

    vi.spyOn(electricityModule, "useElectricityMonitor").mockReturnValue({
      remaining: null,
      observedDate: null,
      expiresAtMs: null,
      expired: true,
      low: false,
    });

    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_oj_snapshot") {
        return { sites: [] };
      }
      if (command === "get_uooc_snapshot") {
        return {
          session: "ready",
          courses: [
            {
              id: "c101",
              title: "大学计算机",
              semester: "2026秋",
              organization: "深大",
              weeks: "10/16",
              endDate: "2026-11-20",
            },
          ],
        };
      }
      if (command === "list_uooc_exams") {
        return [
          {
            id: "exam1",
            title: "期中考试",
            cid: "c101",
            courseName: "大学计算机",
            startTime: "2026-10-25",
            endTime: "2026-10-29 20:00:00",
            status: 1,
            chance: 1,
            submitCount: 0,
            canDo: true,
            canView: false,
            score: null,
          },
        ];
      }
      if (command === "list_uooc_homeworks") {
        return [
          {
            id: "hw1",
            title: "第三次作业",
            cid: "c101",
            courseName: "大学计算机",
            startTime: "2026-10-20",
            endTime: "2026-10-31 23:59:59",
            status: 1,
            chance: 1,
            submitCount: 0,
            canDo: true,
            canView: false,
            score: null,
          },
        ];
      }
      return undefined;
    });

    const { result } = renderHook(() => useHomeDeadlines());
    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.items).toHaveLength(3);

    const courseEnd = result.current.items.find((i) => i.category === "uooc-course-end");
    expect(courseEnd).toBeDefined();
    expect(courseEnd?.title).toBe("大学计算机 课程结课");
    expect(courseEnd?.actionUrl).toBe("/study/uooc?tab=courses");

    const exam = result.current.items.find((i) => i.category === "uooc-exam");
    expect(exam).toBeDefined();
    expect(exam?.title).toBe("期中考试");
    expect(exam?.actionUrl).toBe("/study/uooc?tab=exams&subtab=exam");

    const hw = result.current.items.find((i) => i.category === "uooc-homework");
    expect(hw).toBeDefined();
    expect(hw?.title).toBe("第三次作业");
    expect(hw?.actionUrl).toBe("/study/uooc?tab=exams&subtab=homework");
  });

  it("excludes completed OJ contests where accepted equals total", async () => {
    const now = Date.now();
    vi.spyOn(ojProgressCache, "ensure").mockResolvedValue({ accepted: 5, total: 5 });

    vi.spyOn(electricityModule, "useElectricityMonitor").mockReturnValue({
      remaining: null,
      observedDate: null,
      expiresAtMs: null,
      expired: true,
      low: false,
    });

    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_oj_snapshot") {
        return {
          sites: [
            {
              id: "site1",
              name: "OJ",
              enabled: true,
              session: "ready",
              url: "http://oj",
              username: "u1",
            },
          ],
        };
      }
      if (command === "get_oj_groups") return [{ id: "g1", name: "G1" }];
      if (command === "get_oj_contests") {
        return [
          {
            id: "c1",
            groupId: "g1",
            title: "已完成测验",
            begin: now - 1000,
            end: now + 100000,
            now,
            isExam: false,
          },
        ];
      }
      if (command === "get_uooc_snapshot") return { session: "ready", courses: [] };
      if (command === "list_uooc_exams") return [];
      if (command === "list_uooc_homeworks") return [];
      return undefined;
    });

    const { result } = renderHook(() => useHomeDeadlines());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.items).toHaveLength(0);
  });

  it("shows cached OJ contests before the session becomes ready", async () => {
    const now = Date.now();
    vi.spyOn(electricityModule, "useElectricityMonitor").mockReturnValue({
      remaining: null,
      observedDate: null,
      expiresAtMs: null,
      expired: true,
      low: false,
    });
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_oj_snapshot") {
        return {
          sites: [
            {
              id: "site1",
              name: "OJ",
              enabled: true,
              session: "disabled",
              hasCache: true,
              url: "http://oj",
              username: "student",
            },
          ],
        };
      }
      if (command === "get_oj_groups") return [{ id: "g1", name: "G1" }];
      if (command === "get_oj_contests") {
        return [
          {
            id: "c1",
            groupId: "g1",
            title: "缓存测验",
            begin: now - 1000,
            end: now + 100000,
            now,
            isExam: false,
          },
        ];
      }
      if (command === "get_uooc_snapshot") return { session: "ready", courses: [] };
      if (command === "list_uooc_exams" || command === "list_uooc_homeworks") return [];
      if (command === "get_lms_snapshot") return { session: "ready", todos: [] };
      return undefined;
    });

    const { result } = renderHook(() => useHomeDeadlines());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.items.find((item) => item.category === "oj-quiz")?.title).toBe(
      "缓存测验",
    );
  });

  it("refreshes LMS deadlines when the lms cache changes", async () => {
    const now = Date.now();
    vi.spyOn(electricityModule, "useElectricityMonitor").mockReturnValue({
      remaining: null,
      observedDate: null,
      expiresAtMs: null,
      expired: true,
      low: false,
    });
    let todos: Record<string, unknown>[] = [];
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_oj_snapshot") return { sites: [] };
      if (command === "get_uooc_snapshot") return { session: "ready", courses: [] };
      if (command === "list_uooc_exams" || command === "list_uooc_homeworks") return [];
      if (command === "get_lms_snapshot") return { session: "ready", todos };
      return undefined;
    });

    const { result } = renderHook(() => useHomeDeadlines());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.items).toHaveLength(0);

    todos = [
      {
        id: 1,
        course_id: 9,
        course_name: "数据库",
        title: "第五次作业",
        end_time: new Date(now + 86_400_000).toISOString(),
      },
    ];
    await emit("lms-state-changed", { session: "ready", todos });
    await waitFor(() =>
      expect(result.current.items.find((item) => item.category === "lms-homework")?.title).toBe(
        "第五次作业",
      ),
    );
  });

  it("refreshes OJ deadlines when the oj cache changes", async () => {
    const now = Date.now();
    vi.spyOn(electricityModule, "useElectricityMonitor").mockReturnValue({
      remaining: null,
      observedDate: null,
      expiresAtMs: null,
      expired: true,
      low: false,
    });
    let contests: Record<string, unknown>[] = [];
    let groupCalls = 0;
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_oj_snapshot") {
        return {
          sites: [
            {
              id: "site1",
              name: "OJ",
              enabled: true,
              session: "ready",
              url: "http://oj",
              username: "student",
            },
          ],
        };
      }
      if (command === "get_oj_groups") {
        groupCalls += 1;
        return [{ id: "g1", name: "G1" }];
      }
      if (command === "get_oj_contests") return contests;
      if (command === "get_uooc_snapshot") return { session: "ready", courses: [] };
      if (command === "list_uooc_exams" || command === "list_uooc_homeworks") return [];
      if (command === "get_lms_snapshot") return { session: "ready", todos: [] };
      return undefined;
    });

    const { result } = renderHook(() => useHomeDeadlines());
    await waitFor(() => expect(result.current.loading).toBe(false));
    const before = groupCalls;
    contests = [
      {
        id: "c1",
        groupId: "g1",
        title: "新测验",
        begin: now - 1000,
        end: now + 100000,
        now,
        isExam: false,
      },
    ];
    await emit("oj-cache-changed", "all");
    await waitFor(() =>
      expect(result.current.items.find((item) => item.category === "oj-quiz")?.title).toBe(
        "新测验",
      ),
    );
    expect(groupCalls).toBeGreaterThan(before);
  });
});
