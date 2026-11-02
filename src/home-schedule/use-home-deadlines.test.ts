/** @vitest-environment jsdom */

import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { useHomeDeadlines } from "./use-home-deadlines";
import * as electricityModule from "../electricity-monitor";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class MockChannel {
    onmessage = null;
  },
}));

type ChannelMock = { onmessage: ((message: unknown) => void) | null };

const channels = new Map<string, ChannelMock>();

async function emitChannel(command: string, message: unknown) {
  await act(async () => {
    channels.get(command)?.onmessage?.(message);
  });
}

function installInvoke(handler: (command: string, args: unknown) => unknown = () => undefined) {
  vi.mocked(invoke).mockImplementation((async (command: string, args?: unknown) => {
    if (command.startsWith("subscribe_")) {
      const channel = (args as { channel?: ChannelMock } | undefined)?.channel;
      if (channel) channels.set(command, channel);
      return undefined;
    }
    return handler(command, args);
  }) as typeof invoke);
}

async function waitForChannels() {
  await waitFor(() => {
    expect(channels.get("subscribe_oj_deadlines")).toBeDefined();
    expect(channels.get("subscribe_uooc_state")).toBeDefined();
    expect(channels.get("subscribe_lms_state")).toBeDefined();
    expect(channels.get("subscribe_uooc_assessments")).toBeDefined();
  });
}

type Initials = {
  oj?: unknown[];
  uooc?: unknown;
  lms?: unknown;
  exams?: unknown[];
  homeworks?: unknown[];
};

async function emitInitials(initials: Initials = {}) {
  await emitChannel("subscribe_oj_deadlines", { type: "initial", data: initials.oj ?? [] });
  await emitChannel("subscribe_uooc_state", {
    type: "initial",
    data: initials.uooc ?? { session: "loginRequired", courses: [] },
  });
  await emitChannel("subscribe_lms_state", {
    type: "initial",
    data: initials.lms ?? { session: "needsAuth", todos: [] },
  });
  await emitChannel("subscribe_uooc_assessments", {
    type: "initial",
    data: { exams: initials.exams ?? [], homeworks: initials.homeworks ?? [] },
  });
}

function idleElectricity() {
  vi.spyOn(electricityModule, "useElectricityMonitor").mockReturnValue({
    remaining: null,
    observedDate: null,
    expiresAtMs: null,
    expired: true,
    low: false,
  });
}

describe("useHomeDeadlines", () => {
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  beforeEach(() => {
    channels.clear();
    installInvoke();
  });

  it("aggregates active OJ contest deadline and electricity outage forecast", async () => {
    const now = new Date("2026-10-28T10:00:00Z").getTime();
    vi.spyOn(Date, "now").mockReturnValue(now);

    vi.spyOn(electricityModule, "useElectricityMonitor").mockReturnValue({
      remaining: 8.5,
      observedDate: "2026-10-28",
      expiresAtMs: now + 3600_000,
      expired: false,
      low: false,
      estimatedDaysLeft: 3.5, // outage in 3.5 days -> 2026-10-31
    });

    const { result } = renderHook(() => useHomeDeadlines());
    await waitForChannels();
    await emitInitials({
      oj: [
        {
          siteId: "site1",
          siteName: "实验OJ",
          groupId: "g1",
          id: "c1",
          title: "算法测验 1",
          begin: now - 3600_000,
          end: now + 86400_000 * 2,
          now,
          isExam: false,
          accepted: 0,
          total: 3,
          hasProgress: true,
        },
      ],
    });
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

  it("aggregates UOOC course end date, exam deadline, and homework deadline from channels", async () => {
    const now = new Date("2026-10-28T10:00:00Z").getTime();
    vi.spyOn(Date, "now").mockReturnValue(now);
    idleElectricity();

    const { result } = renderHook(() => useHomeDeadlines());
    await waitForChannels();
    await emitInitials({
      uooc: {
        revision: 1,
        session: "ready",
        user: { id: "7", name: "测试用户" },
        settings: {},
        tasks: [],
        message: "",
        logs: [],
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
      },
      exams: [
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
      ],
      homeworks: [
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
      ],
    });

    await waitFor(() => expect(result.current.loading).toBe(false));
    await waitFor(() => expect(result.current.items).toHaveLength(3));

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

  it("does not poll snapshot or list commands from the home calendar", async () => {
    idleElectricity();
    const { unmount } = renderHook(() => useHomeDeadlines());
    await waitForChannels();
    unmount();

    expect(invoke).not.toHaveBeenCalledWith("get_uooc_snapshot", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("list_uooc_exams", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("list_uooc_homeworks", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("get_oj_snapshot", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("get_oj_groups", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("get_oj_contests", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("get_lms_snapshot", expect.anything());
  });

  it("excludes completed OJ contests where accepted equals total", async () => {
    const now = Date.now();
    vi.spyOn(Date, "now").mockReturnValue(now);
    idleElectricity();

    const { result } = renderHook(() => useHomeDeadlines());
    await waitForChannels();
    await emitInitials({
      oj: [
        {
          siteId: "site1",
          siteName: "OJ",
          groupId: "g1",
          id: "c1",
          title: "已完成测验",
          begin: now - 1000,
          end: now + 100000,
          now,
          isExam: false,
          accepted: 5,
          total: 5,
          hasProgress: true,
        },
      ],
    });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.items).toHaveLength(0);
  });

  it("shows OJ deadlines pushed from the backend cache", async () => {
    const now = Date.now();
    idleElectricity();

    const { result } = renderHook(() => useHomeDeadlines());
    await waitForChannels();
    await emitInitials({
      oj: [
        {
          siteId: "site1",
          siteName: "OJ",
          groupId: "g1",
          id: "c1",
          title: "缓存测验",
          begin: now - 1000,
          end: now + 100000,
          now,
          isExam: false,
          accepted: 0,
          total: 0,
          hasProgress: false,
        },
      ],
    });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.items.find((item) => item.category === "oj-quiz")?.title).toBe(
      "缓存测验",
    );
  });

  it("updates LMS deadlines when the lms channel pushes a snapshot", async () => {
    const now = Date.now();
    idleElectricity();

    const { result } = renderHook(() => useHomeDeadlines());
    await waitForChannels();
    await emitInitials();
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.items).toHaveLength(0);

    await emitChannel("subscribe_lms_state", {
      type: "snapshot",
      data: {
        session: "ready",
        todos: [
          {
            id: 1,
            course_id: 9,
            course_name: "数据库",
            title: "第五次作业",
            end_time: new Date(now + 86_400_000).toISOString(),
          },
        ],
      },
    });
    await waitFor(() =>
      expect(result.current.items.find((item) => item.category === "lms-homework")?.title).toBe(
        "第五次作业",
      ),
    );
  });

  it("updates OJ deadlines when the oj channel pushes a snapshot", async () => {
    const now = Date.now();
    idleElectricity();

    const { result } = renderHook(() => useHomeDeadlines());
    await waitForChannels();
    await emitInitials();
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.items).toHaveLength(0);

    await emitChannel("subscribe_oj_deadlines", {
      type: "snapshot",
      data: [
        {
          siteId: "site1",
          siteName: "OJ",
          groupId: "g1",
          id: "c1",
          title: "新测验",
          begin: now - 1000,
          end: now + 100000,
          now,
          isExam: false,
          accepted: 0,
          total: 0,
          hasProgress: false,
        },
      ],
    });
    await waitFor(() =>
      expect(result.current.items.find((item) => item.category === "oj-quiz")?.title).toBe(
        "新测验",
      ),
    );
  });
});
