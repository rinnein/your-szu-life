import { describe, expect, it } from "vite-plus/test";
import { PERIOD_BLOCKS, periodLabel, rowTimeLabel, slotForSession } from "./periods";

function at(hour: number, minute: number): number {
  return new Date(2026, 10, 16, hour, minute, 0, 0).getTime();
}

describe("timetable periods", () => {
  it("maps a single-period class to its standard block", () => {
    const slot = slotForSession(at(8, 30), at(9, 10));
    expect(slot).toBeDefined();
    expect(slot?.row.id).toBe("lesson-1");
    expect(slot?.periodLabel).toBe("第1节");
    expect(slot?.timeLabel).toBe("08:30–09:10");
    expect(rowTimeLabel(slot!.row)).toBe("08:30–09:10");
  });

  it("crosses the break for a two-period class", () => {
    const slot = slotForSession(at(8, 30), at(10, 5));
    expect(slot?.row.id).toBe("lesson-1");
    expect(slot?.periodLabel).toBe("第1–2节");
    expect(slot?.timeLabel).toBe("08:30–10:05");
  });

  it("accepts a 15-minute drift from the attendance API", () => {
    const slot = slotForSession(at(8, 40), at(9, 20));
    expect(slot?.row.id).toBe("lesson-1");
  });

  it("returns undefined for times outside the standard periods", () => {
    expect(slotForSession(at(7, 0), at(8, 0))).toBeUndefined();
    expect(slotForSession(at(22, 0), at(23, 0))).toBeUndefined();
  });

  it("keeps the evening blocks merged", () => {
    const slot = slotForSession(at(19, 0), at(20, 20));
    expect(slot?.row.id).toBe("lessons-11-12");
    expect(slot?.periodLabel).toBe("第11–12节");
    expect(PERIOD_BLOCKS.some((row) => row.id === "morning-break")).toBe(false);
    expect(periodLabel(PERIOD_BLOCKS[0])).toBe("第1节");
  });
});
