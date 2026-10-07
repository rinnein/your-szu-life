import { describe, it, expect } from "vite-plus/test";
import {
  defaultMode,
  nearSchoolRestart,
  academicMatch,
  academicStatus,
  timetable,
  conflicts,
  type Course,
  type Academic,
} from "./model";
const course = (id: string, schedule: string): Course => ({
  id,
  schedule,
  code: id,
  name: id,
  teacher: "",
  category: "",
  credits: 2,
  remaining: null,
  capacity: null,
  campus: "01",
  hasTest: false,
  testIds: [],
  mainSelect: true,
});
describe("enrollment decisions", () => {
  it("defaults to campus on timeout and favors a reachable lower latency", () => {
    expect(defaultMode([])).toBe("campus");
    expect(
      defaultMode([
        { mode: "webvpn", needs2fa: true, latency: 1, message: "" },
        { mode: "campus", needs2fa: false, latency: 100, message: "" },
      ]),
    ).toBe("campus");
    expect(
      defaultMode([
        { mode: "webvpn", needs2fa: false, latency: 20, message: "" },
        { mode: "campus", needs2fa: false, latency: 100, message: "" },
      ]),
    ).toBe("webvpn");
  });
  it("does not match same names with different course codes", () => {
    const a = {
      modules: [{ module: {}, courses: [{ KCH: "a", KCM: "相同名称", SFTG: "1" }] }],
    } as unknown as Academic;
    expect(academicMatch(a, "b")).toBeUndefined();
    expect(academicStatus(academicMatch(a, "a"))).toBe("已完成");
  });
  it("preserves unknown completion and does not mark all electives required", () => {
    expect(academicStatus({ SFTG: "2", KCXZDM_DISPLAY: "选修" })).toBe("未修 · 课组候选");
    expect(academicStatus({ SFTG: "" })).toBe("学业状态未知");
  });
  it("checks week parity and preserves unparsed schedules", () => {
    const { slots, unscheduled } = timetable(
      [
        course("a", "1-16周(单) 星期一 1-2节"),
        course("b", "1-16周(双) 星期一 1-2节"),
        course("c", "暂未安排"),
      ],
      [],
    );
    expect(slots).toHaveLength(2);
    expect(conflicts(slots[0], slots[1])).toBe(false);
    expect(unscheduled).toHaveLength(1);
  });
});

it("uses school timezone for restart hints without expiring sessions by clock", () => {
  expect(nearSchoolRestart(Date.parse("2026-10-07T07:59:00Z"))).toBe(true);
  expect(nearSchoolRestart(Date.parse("2026-10-07T12:04:00Z"))).toBe(true);
  expect(nearSchoolRestart(Date.parse("2026-10-07T13:00:00Z"))).toBe(false);
});

it("retains disjoint week ranges and multiple weekdays", () => {
  const { slots } = timetable([course("a", "2-4周,6-10周(双) 星期一、三 1-2节")], []);
  expect(slots.map((s) => s.day)).toEqual([1, 3]);
  expect(slots[0].weeks).toEqual([2, 3, 4, 6, 8, 10]);
});
