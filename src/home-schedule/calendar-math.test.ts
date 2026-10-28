/** @vitest-environment jsdom */

import { describe, expect, it } from "vite-plus/test";
import {
  formatDateKey,
  getDaysInMonth,
  getFirstDayOfMonth,
  generateCalendarGrid,
} from "./calendar-math";
import type { ScheduleCategory } from "./types";

describe("calendar-math", () => {
  it("formats date to YYYY-MM-DD correctly", () => {
    const date = new Date(2026, 9, 28); // Month is 0-indexed: 9 = October
    expect(formatDateKey(date)).toBe("2026-10-28");
  });

  it("calculates days in month correctly including leap years", () => {
    expect(getDaysInMonth(2026, 1)).toBe(28); // Feb 2026
    expect(getDaysInMonth(2024, 1)).toBe(29); // Feb 2024 (leap year)
    expect(getDaysInMonth(2026, 9)).toBe(31); // Oct 2026
  });

  it("calculates weekday of first day of month correctly", () => {
    // 2026-10-01 is a Thursday (index 4: Sun=0, Mon=1, Tue=2, Wed=3, Thu=4, Fri=5, Sat=6)
    expect(getFirstDayOfMonth(2026, 9)).toBe(4);
  });

  it("generates exactly 42 grid cells with correct leading and trailing days", () => {
    const categoryMap = new Map<string, ScheduleCategory[]>([
      ["2026-10-28", ["oj-quiz"]],
      ["2026-10-31", ["electricity-outage"]],
    ]);

    const grid = generateCalendarGrid(2026, 9, "2026-10-28", categoryMap);
    expect(grid).toHaveLength(42);

    // 2026-10-01 starts on Thursday (index 4), so first 4 cells are from September (27, 28, 29, 30)
    expect(grid[0].isCurrentMonth).toBe(false);
    expect(grid[0].day).toBe(27);
    expect(grid[3].day).toBe(30);

    // 5th cell (index 4) is Oct 1st
    expect(grid[4].isCurrentMonth).toBe(true);
    expect(grid[4].day).toBe(1);

    // Oct 28th should be selected and have oj-quiz category
    const oct28 = grid.find((cell) => cell.dateKey === "2026-10-28");
    expect(oct28).toBeDefined();
    expect(oct28?.isCurrentMonth).toBe(true);
    expect(oct28?.isSelected).toBe(true);
    expect(oct28?.categories).toEqual(["oj-quiz"]);

    // Oct 31st should have electricity-outage
    const oct31 = grid.find((cell) => cell.dateKey === "2026-10-31");
    expect(oct31?.categories).toEqual(["electricity-outage"]);
  });
});
