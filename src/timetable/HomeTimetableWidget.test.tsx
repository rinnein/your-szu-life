/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vite-plus/test";
import { HomeTimetableWidget } from "./HomeTimetableWidget";

const { nav, snapshot } = vi.hoisted(() => {
  const today = new Date();
  const date = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, "0")}-${String(
    today.getDate(),
  ).padStart(2, "0")}`;
  const start = new Date(today);
  start.setHours(8, 30, 0, 0);
  const end = new Date(today);
  end.setHours(10, 5, 0, 0);
  return {
    nav: { push: vi.fn(), replace: vi.fn(), back: vi.fn() },
    snapshot: {
      fetchedAtMs: Date.now(),
      semester: "2026 秋",
      message: "",
      sessions: [
        {
          date,
          courseName: "编译原理",
          buildingName: "致理楼",
          classroom: "301",
          buildingNameEn: null,
          startMs: start.getTime(),
          endMs: end.getTime(),
          semester: "2026 秋",
          checkin: "pending_lms",
        },
      ],
      checkins: [
        {
          id: "checkin:lms:1",
          title: "编译原理 签到",
          subtitle: null,
          firstSeenMs: Date.now() - 60_000,
          route: "/study/signin?tab=lms",
        },
      ],
      semesterCourses: [
        {
          name: "编译原理",
          teacher: "演示教师",
          classroom: "demo-room-1",
          weekday: 1,
          startPeriod: 1,
          endPeriod: 2,
          weeks: [1, 2, 3],
          rawWeeks: "1-3周",
        },
      ],
      semesterFetchedAtMs: Date.now(),
      semesterMessage: "",
    },
  };
});

vi.mock("../logical-navigation", () => ({
  useLogicalNavigation: () => nav,
}));

vi.mock("./use-timetable", () => ({
  useTimetable: () => ({
    snapshot,
    error: undefined,
    refresh: vi.fn(),
    refreshing: false,
  }),
}));

describe("HomeTimetableWidget", () => {
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("shows today's class with period, time and location", () => {
    render(<HomeTimetableWidget />);
    expect(screen.getByText("课表")).toBeInTheDocument();
    expect(screen.getByText("编译原理")).toBeInTheDocument();
    expect(screen.getByText(/第1–2节/)).toBeInTheDocument();
    expect(screen.getByText(/致理楼 301/)).toBeInTheDocument();
  });

  it("shows the pending check-in label and opens the sign-in page", () => {
    snapshot.sessions[0].checkin = "pending_lms";
    render(<HomeTimetableWidget />);
    expect(screen.getByText("待签到(畅课lms)")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /编译原理/ }));
    expect(nav.push).toHaveBeenCalledWith(
      expect.stringMatching(/^\/study\/signin\?tab=lms&course=.*&open=1$/),
    );
  });

  it("routes a class without a check-in task to the detailed timetable", () => {
    snapshot.sessions[0].checkin = "none";
    render(<HomeTimetableWidget />);
    expect(screen.queryByText("待签到(畅课lms)")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /编译原理/ }));
    expect(nav.push).toHaveBeenCalledWith(
      expect.stringMatching(/^\/study\/timetable\?tab=week&date=/),
    );
  });

  it("does not double count SZU check-in items in the unmatched button", () => {
    const previous = snapshot.checkins;
    snapshot.sessions[0].checkin = "pending_lms";
    snapshot.checkins = [
      ...previous,
      {
        id: "checkin:szu:编译原理|2026-11-17|1000",
        title: "编译原理 签到",
        subtitle: null,
        firstSeenMs: Date.now(),
        route: "/study/signin?tab=szu",
      },
    ];
    render(<HomeTimetableWidget />);
    // SZU 待签已由课次 chip 表达，不应再出现在「另有 n 个待签点名」里。
    expect(screen.queryByRole("button", { name: /另有/ })).not.toBeInTheDocument();
    snapshot.checkins = previous;
    snapshot.sessions[0].checkin = "none";
  });
});
