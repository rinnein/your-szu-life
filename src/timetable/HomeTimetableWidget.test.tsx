/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vite-plus/test";
import { HomeTimetableWidget } from "./HomeTimetableWidget";
import type { TimetableSnapshot } from "./types";

const { nav, snapshot, NOW } = vi.hoisted(() => {
  const today = new Date();
  const date = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, "0")}-${String(
    today.getDate(),
  ).padStart(2, "0")}`;
  const start = new Date(today);
  start.setHours(8, 30, 0, 0);
  const end = new Date(today);
  end.setHours(10, 5, 0, 0);
  const now = new Date(today);
  now.setHours(9, 0, 0, 0);
  const snapshot: TimetableSnapshot = {
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
  };
  return {
    NOW: now.getTime(),
    nav: { push: vi.fn(), replace: vi.fn(), back: vi.fn() },
    snapshot,
  };
});

vi.mock("../logical-navigation", () => ({
  useLogicalNavigation: () => nav,
}));

// 固定「现在」为今天的 09:00：8:30-10:05 的课处于上课中，避免测试随时段漂移。
function renderWidget() {
  return render(
    <HomeTimetableWidget snapshot={snapshot} refreshing={false} refresh={vi.fn()} now={NOW} />,
  );
}

describe("HomeTimetableWidget", () => {
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("shows today's class with period, time and location", () => {
    renderWidget();
    expect(screen.getByText("课表")).toBeInTheDocument();
    expect(screen.getByText("编译原理")).toBeInTheDocument();
    expect(screen.getByText(/第1–2节/)).toBeInTheDocument();
    expect(screen.getByText(/致理楼 301/)).toBeInTheDocument();
  });

  it("shows the pending check-in label and opens the sign-in page", () => {
    snapshot.sessions[0].checkin = "pending_lms";
    renderWidget();
    expect(screen.getByText("待签到(畅课lms)")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /编译原理/ }));
    expect(nav.push).toHaveBeenCalledWith(
      expect.stringMatching(/^\/study\/signin\?tab=lms&course=.*&open=1$/),
    );
  });

  it("routes a class without a check-in task to the detailed timetable", () => {
    snapshot.sessions[0].checkin = "none";
    renderWidget();
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
    renderWidget();
    // SZU 待签已由课次 chip 表达，不应再出现在「另有 n 个待签点名」里。
    expect(screen.queryByRole("button", { name: /另有/ })).not.toBeInTheDocument();
    snapshot.checkins = previous;
    snapshot.sessions[0].checkin = "none";
  });

  it("collapses classes that have already finished", () => {
    const session = snapshot.sessions[0];
    const previous = { startMs: session.startMs, endMs: session.endMs, checkin: session.checkin };
    const start = new Date(NOW);
    start.setHours(7, 0, 0, 0);
    const end = new Date(NOW);
    end.setHours(8, 0, 0, 0);
    session.startMs = start.getTime();
    session.endMs = end.getTime();
    session.checkin = "none";

    try {
      renderWidget();
      // 默认收起：已上完的课程不渲染，只留一行可展开的标题。
      expect(screen.queryByText("编译原理")).not.toBeInTheDocument();
      const trigger = screen.getByRole("button", { name: "展开已上过的课" });
      expect(trigger).toHaveAttribute("aria-expanded", "false");

      fireEvent.click(trigger);
      expect(trigger).toHaveAttribute("aria-expanded", "true");
      expect(screen.getByText("编译原理")).toBeInTheDocument();
    } finally {
      session.startMs = previous.startMs;
      session.endMs = previous.endMs;
      session.checkin = previous.checkin;
    }
  });

  it("keeps finished classes above the current and upcoming ones", () => {
    const original = snapshot.sessions;
    const template = original[0];
    const start = (hour: number) => {
      const date = new Date(NOW);
      date.setHours(hour, 0, 0, 0);
      return date.getTime();
    };
    snapshot.sessions = [
      { ...template, courseName: "编译原理", startMs: start(7), endMs: start(8), checkin: "none" },
      { ...template, courseName: "离散数学", startMs: start(8), endMs: start(10), checkin: "none" },
    ];

    try {
      renderWidget();
      const trigger = screen.getByRole("button", { name: "展开已上过的课" });
      const active = screen.getByRole("button", { name: /离散数学/ });
      // 折叠区（已上过）应排在正在上/未上的课之前，展开后仍是时间顺序。
      expect(
        trigger.compareDocumentPosition(active) & Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();

      fireEvent.click(trigger);
      const finished = screen.getByRole("button", { name: /编译原理/ });
      expect(
        finished.compareDocumentPosition(active) & Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
    } finally {
      snapshot.sessions = original;
    }
  });

  it("shows a skeleton instead of the empty state before the first snapshot", () => {
    render(
      <HomeTimetableWidget snapshot={undefined} refreshing={false} refresh={vi.fn()} now={NOW} />,
    );

    expect(screen.getByRole("status", { name: "正在读取课表" })).toBeInTheDocument();
    expect(screen.queryByText("今天没有课。")).not.toBeInTheDocument();
    expect(screen.queryByText(/今天还剩/)).not.toBeInTheDocument();
  });
});
