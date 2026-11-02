/** @vitest-environment jsdom */

import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { useHomeDeadlines } from "./use-home-deadlines";

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

async function waitForChannel() {
  await waitFor(() => expect(channels.get("subscribe_todos")).toBeDefined());
}

function todoItem(overrides: Record<string, unknown> = {}) {
  return {
    id: "oj:site1:c1",
    kind: "oj-quiz",
    title: "算法测验 1",
    subtitle: "实验OJ · 测验",
    dueAtMs: Date.now() + 86_400_000,
    route: "/study/oj?site=site1&contest=c1",
    externalUrl: null,
    urgent: false,
    ...overrides,
  };
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

  it("maps the aggregated backend todo list into calendar items", async () => {
    const now = new Date("2026-10-28T10:00:00Z").getTime();
    vi.spyOn(Date, "now").mockReturnValue(now);

    const { result } = renderHook(() => useHomeDeadlines());
    await waitForChannel();
    await emitChannel("subscribe_todos", {
      type: "initial",
      data: [
        todoItem({
          id: "electricity:outage-forecast",
          kind: "electricity-outage",
          title: "预计电量耗尽（剩余约 8.5 度）",
          subtitle: "宿舍电费监控 · 预估断电日期",
          dueAtMs: now + 3.5 * 86_400_000,
          route: "/life/campus-payments/recharge",
        }),
        todoItem(),
      ],
    });

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.items).toHaveLength(2);
    // 后端已排序，前端仍按截止时间兜底排序。
    expect(result.current.items.map((item) => item.category)).toEqual([
      "oj-quiz",
      "electricity-outage",
    ]);

    const ojItem = result.current.items[0];
    expect(ojItem.title).toBe("算法测验 1");
    expect(ojItem.subtitle).toBe("实验OJ · 测验");
    expect(ojItem.actionUrl).toContain("/study/oj?site=site1&contest=c1");
    expect(ojItem.dateKey).toBe("2026-10-29");

    const elecItem = result.current.items[1];
    expect(elecItem.actionUrl).toBe("/life/campus-payments/recharge");
    expect(result.current.categoryMap.get(ojItem.dateKey)).toContain("oj-quiz");
  });

  it("keeps external links for LMS todos", async () => {
    const { result } = renderHook(() => useHomeDeadlines());
    await waitForChannel();
    await emitChannel("subscribe_todos", {
      type: "initial",
      data: [
        todoItem({
          id: "lms:homework:1",
          kind: "lms-homework",
          title: "第五次作业",
          subtitle: "畅课 · 数据库",
          route: "/study/lms",
          externalUrl: "https://lms.szu.edu.cn/course/9/learning-activity#/1?view=scores",
        }),
      ],
    });

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.items[0].externalUrl).toBe(
      "https://lms.szu.edu.cn/course/9/learning-activity#/1?view=scores",
    );
  });

  it("refreshes through the todos command only", async () => {
    const { result } = renderHook(() => useHomeDeadlines());
    await waitForChannel();
    await emitChannel("subscribe_todos", { type: "initial", data: [] });
    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await result.current.refresh();
    });
    expect(invoke).toHaveBeenCalledWith("refresh_todos");
  });

  it("does not poll module snapshot commands from the home calendar", async () => {
    const { unmount } = renderHook(() => useHomeDeadlines());
    await waitForChannel();
    unmount();

    expect(invoke).not.toHaveBeenCalledWith("get_uooc_snapshot", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("get_oj_snapshot", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("get_lms_snapshot", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("get_electricity_status", expect.anything());
  });

  it("stops loading when the subscription fails", async () => {
    installInvoke(() => {
      throw new Error("boom");
    });
    vi.mocked(invoke).mockRejectedValue(new Error("boom"));

    const { result } = renderHook(() => useHomeDeadlines());
    await waitFor(() => expect(result.current.loading).toBe(false));
  });
});
