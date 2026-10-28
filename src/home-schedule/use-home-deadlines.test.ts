/** @vitest-environment jsdom */

import { cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useHomeDeadlines } from "./use-home-deadlines";
import * as electricityModule from "../electricity-monitor";
import { ojProgressCache } from "../oj/progress-store";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

describe("useHomeDeadlines", () => {
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  beforeEach(() => {
    vi.mocked(listen).mockResolvedValue(() => undefined);
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

    // Mock OJ commands
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
      return undefined;
    });

    const { result } = renderHook(() => useHomeDeadlines());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.items).toHaveLength(0);
  });
});
