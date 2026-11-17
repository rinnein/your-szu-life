/** @vitest-environment jsdom */
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { useTimetable } from "./use-timetable";
import type { TimetableSnapshot } from "./types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class MockChannel {
    onmessage = null;
  },
}));

type ChannelMock = { onmessage: ((message: unknown) => void) | null };

const channels = new Map<string, ChannelMock>();

function installInvoke() {
  vi.mocked(invoke).mockImplementation((async (command: string, args?: unknown) => {
    if (command === "subscribe_timetable") {
      const channel = (args as { channel?: ChannelMock } | undefined)?.channel;
      if (channel) channels.set(command, channel);
      return undefined;
    }
    return undefined;
  }) as typeof invoke);
}

async function waitForChannel() {
  await waitFor(() => expect(channels.get("subscribe_timetable")).toBeDefined());
}

async function emit(event: unknown) {
  await act(async () => {
    channels.get("subscribe_timetable")?.onmessage?.(event);
  });
}

function snapshotWith(courseName: string): TimetableSnapshot {
  return {
    fetchedAtMs: 1,
    semester: null,
    message: "",
    sessions: [
      {
        date: "2026-11-17",
        courseName,
        buildingName: null,
        buildingNameEn: null,
        classroom: null,
        startMs: 1,
        endMs: 2,
        semester: null,
        checkin: "none",
      },
    ],
    checkins: [],
    semesterCourses: [],
    semesterFetchedAtMs: 0,
    semesterMessage: "",
  };
}

describe("useTimetable cache", () => {
  beforeEach(() => {
    channels.clear();
    installInvoke();
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("hydrates the last snapshot for the same account after remount", async () => {
    const first = renderHook(() => useTimetable("cache-timetable-a"));
    await waitForChannel();
    await emit({ type: "initial", data: snapshotWith("编译原理") });
    await waitFor(() =>
      expect(first.result.current.snapshot?.sessions[0].courseName).toBe("编译原理"),
    );
    first.unmount();

    channels.clear();
    const second = renderHook(() => useTimetable("cache-timetable-a"));

    // 首帧就命中缓存，不需要等 Channel 再来一次。
    expect(second.result.current.snapshot?.sessions[0].courseName).toBe("编译原理");
  });

  it("does not reuse another account's cached snapshot", () => {
    const { result } = renderHook(() => useTimetable("cache-timetable-b"));

    expect(result.current.snapshot).toBeUndefined();
  });
});
