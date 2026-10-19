/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { useActiveOjContests } from "./active-contests";
import { ojProgressCache } from "./progress-store";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

beforeEach(() => {
  vi.clearAllMocks();
  ojProgressCache.clear();
});

afterEach(() => {
  cleanup();
});

describe("useActiveOjContests", () => {
  it("returns active unfinished contests for enabled ready sites", async () => {
    const now = Date.now();
    vi.mocked(invoke).mockImplementation(async (command, args: any) => {
      if (command === "get_oj_snapshot") {
        return {
          sites: [
            {
              id: "szu-oj",
              name: "深大 OJ",
              url: "http://oj.test/",
              builtin: true,
              reminders: true,
              enabled: true,
              username: "student",
              session: "ready",
              message: "",
            },
            {
              id: "disabled-oj",
              name: "未启用 OJ",
              url: "http://oj.test2/",
              builtin: false,
              reminders: false,
              enabled: false,
              username: "",
              session: "disabled",
              message: "",
            },
          ],
          notices: [],
          companionPort: 27121,
          desktop: true,
        };
      }
      if (command === "get_oj_groups") {
        return [{ id: "group-1", name: "计算机一班" }];
      }
      if (command === "get_oj_contests") {
        return [
          // Ongoing & unfinished contest
          {
            id: "contest-running-incomplete",
            groupId: "group-1",
            title: "进行中未完成测验",
            description: "",
            begin: now - 3600000,
            end: now + 3600000,
            now,
            isExam: false,
          },
          // Ongoing & completed contest
          {
            id: "contest-running-complete",
            groupId: "group-1",
            title: "进行中已完成测验",
            description: "",
            begin: now - 3600000,
            end: now + 3600000,
            now,
            isExam: false,
          },
          // Ended contest
          {
            id: "contest-ended",
            groupId: "group-1",
            title: "已结束测验",
            description: "",
            begin: now - 7200000,
            end: now - 3600000,
            now,
            isExam: false,
          },
          // Future contest
          {
            id: "contest-future",
            groupId: "group-1",
            title: "未开始测验",
            description: "",
            begin: now + 3600000,
            end: now + 7200000,
            now,
            isExam: false,
          },
        ];
      }
      if (command === "get_oj_progress") {
        if (args?.contestId === "contest-running-complete") {
          return { accepted: 2, total: 2, updatedAt: now };
        }
        if (args?.contestId === "contest-running-incomplete") {
          return { accepted: 1, total: 2, updatedAt: now };
        }
      }
      return undefined;
    });

    const { result } = renderHook(() => useActiveOjContests());

    await waitFor(() => {
      expect(result.current).toHaveLength(1);
    });

    expect(result.current[0]).toMatchObject({
      siteId: "szu-oj",
      siteName: "深大 OJ",
      contestId: "contest-running-incomplete",
      title: "进行中未完成测验",
      url: "/oj?site=szu-oj&contest=contest-running-incomplete",
    });
  });

  it("returns empty list when all contests are completed or no sites enabled", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_oj_snapshot") {
        return {
          sites: [],
          notices: [],
          companionPort: 27121,
          desktop: true,
        };
      }
      return undefined;
    });

    const { result } = renderHook(() => useActiveOjContests());
    await waitFor(() => expect(result.current).toEqual([]));
  });
});
