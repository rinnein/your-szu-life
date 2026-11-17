import { describe, expect, it } from "vite-plus/test";
import { resolveHomeCardOrder } from "./home-card-order";

const NOW = new Date(2026, 10, 17, 18, 0, 0).getTime();

function orderOf(
  overrides: Partial<{
    todaySessions: readonly { endMs: number }[];
    now: number;
    hasTodos: boolean;
    ready: boolean;
  }>,
) {
  return resolveHomeCardOrder({
    todaySessions: [],
    now: NOW,
    hasTodos: true,
    ready: true,
    ...overrides,
  });
}

describe("resolveHomeCardOrder", () => {
  it("keeps the timetable on top before both snapshots have arrived", () => {
    expect(orderOf({ ready: false, todaySessions: [] })).toBe("timetable");
    expect(orderOf({ ready: false, todaySessions: [{ endMs: NOW - 60_000 }] })).toBe("timetable");
  });

  it("keeps the timetable on top when there is nothing to do", () => {
    expect(orderOf({ hasTodos: false })).toBe("timetable");
    expect(orderOf({ hasTodos: false, todaySessions: [{ endMs: NOW - 60_000 }] })).toBe(
      "timetable",
    );
  });

  it("moves the calendar on top when today has no classes", () => {
    expect(orderOf({ todaySessions: [] })).toBe("calendar");
  });

  it("keeps the timetable on top while a class is ongoing", () => {
    expect(orderOf({ todaySessions: [{ endMs: NOW + 60_000 }] })).toBe("timetable");
  });

  it("keeps the timetable on top while a class is still upcoming", () => {
    expect(orderOf({ todaySessions: [{ endMs: NOW + 3 * 60 * 60_000 }] })).toBe("timetable");
  });

  it("moves the calendar on top after the last class of the day ends", () => {
    expect(
      orderOf({
        todaySessions: [{ endMs: NOW - 3 * 60 * 60_000 }, { endMs: NOW - 60_000 }],
      }),
    ).toBe("calendar");
  });

  it("treats a class ending exactly now as finished", () => {
    expect(orderOf({ todaySessions: [{ endMs: NOW }] })).toBe("calendar");
  });

  it("keeps the timetable on top when any class today is still running", () => {
    expect(
      orderOf({
        todaySessions: [{ endMs: NOW - 60_000 }, { endMs: NOW + 60_000 }],
      }),
    ).toBe("timetable");
  });
});
