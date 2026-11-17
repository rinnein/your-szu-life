/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { HomeCards } from "./HomeCards";
import { localDateKey } from "../timetable/merge";
import type { TimetableSession, TimetableSnapshot } from "../timetable/types";
import type { ScheduleItem } from "./types";

const NOW = new Date(2026, 10, 17, 18, 0, 0).getTime();

vi.mock("../logical-navigation", () => ({
  useLogicalNavigation: () => ({ push: vi.fn(), replace: vi.fn(), back: vi.fn() }),
}));

function session(courseName: string, startMs: number, endMs: number): TimetableSession {
  return {
    date: localDateKey(new Date(NOW)),
    courseName,
    buildingName: null,
    buildingNameEn: null,
    classroom: null,
    startMs,
    endMs,
    semester: null,
    checkin: "none",
  };
}

function snapshotWith(sessions: TimetableSession[]): TimetableSnapshot {
  return {
    fetchedAtMs: NOW,
    semester: null,
    message: "",
    sessions,
    checkins: [],
    semesterCourses: [],
    semesterFetchedAtMs: NOW,
    semesterMessage: "",
  };
}

const todo: ScheduleItem = {
  id: "oj:1",
  category: "oj-quiz",
  title: "算法测验",
  subtitle: "OJ",
  targetDate: new Date(NOW + 60 * 60_000),
  dateKey: localDateKey(new Date(NOW)),
  actionUrl: "/study/oj",
};

function renderCards({
  sessions,
  hasTodos = true,
  ready = true,
}: {
  sessions: TimetableSession[];
  hasTodos?: boolean;
  ready?: boolean;
}) {
  return render(
    <HomeCards
      now={NOW}
      timetable={{ snapshot: snapshotWith(sessions), refreshing: false, refresh: vi.fn() }}
      calendar={{
        items: hasTodos ? [todo] : [],
        categoryMap: new Map(),
        loading: false,
        ready,
        refresh: vi.fn(),
      }}
    />,
  );
}

function expectTimetableAboveCalendar() {
  const timetable = screen.getByRole("heading", { name: "课表" });
  const calendar = screen.getByRole("heading", { name: "日历与待办" });
  expect(
    timetable.compareDocumentPosition(calendar) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy();
}

function expectCalendarAboveTimetable() {
  const timetable = screen.getByRole("heading", { name: "课表" });
  const calendar = screen.getByRole("heading", { name: "日历与待办" });
  expect(
    calendar.compareDocumentPosition(timetable) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy();
}

describe("HomeCards", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
    vi.useRealTimers();
  });

  it("keeps the timetable on top until both snapshots are ready", () => {
    renderCards({
      sessions: [session("编译原理", NOW - 2 * 60 * 60_000, NOW - 60_000)],
      ready: false,
    });
    expectTimetableAboveCalendar();
  });

  it("keeps the timetable on top when there are no todos", () => {
    renderCards({
      sessions: [session("编译原理", NOW - 2 * 60 * 60_000, NOW - 60_000)],
      hasTodos: false,
    });
    expectTimetableAboveCalendar();
  });

  it("moves the calendar on top when today has no classes", () => {
    renderCards({ sessions: [] });
    expectCalendarAboveTimetable();
  });

  it("keeps the timetable on top while a class is ongoing", () => {
    renderCards({ sessions: [session("编译原理", NOW - 60_000, NOW + 60 * 60_000)] });
    expectTimetableAboveCalendar();
  });

  it("moves the calendar on top after the last class of the day ends", () => {
    renderCards({
      sessions: [
        session("编译原理", NOW - 4 * 60 * 60_000, NOW - 2 * 60 * 60_000),
        session("离散数学", NOW - 2 * 60 * 60_000, NOW - 60_000),
      ],
    });
    expectCalendarAboveTimetable();
  });
});
