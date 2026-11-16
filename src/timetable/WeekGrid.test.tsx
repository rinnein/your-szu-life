/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { WeekGrid } from "./WeekGrid";
import type { TimetableSession } from "./types";
import { localDateKey } from "./merge";

const nav = { push: vi.fn(), replace: vi.fn(), back: vi.fn() };
vi.mock("../logical-navigation", () => ({
  useLogicalNavigation: () => nav,
}));

// 课程状态取决于当前时间，测试里固定成「今天 08:00」，避免在 08:30–09:10
// 之间跑测试时状态从“未来”变成“上课中”而让断言抖动。
const clock = vi.hoisted(() => ({ now: 0 }));
vi.mock("./use-now", () => ({ useNow: () => clock.now }));

function atToday(hour: number, minute: number): number {
  const date = new Date();
  date.setHours(hour, minute, 0, 0);
  return date.getTime();
}

function session(overrides: Partial<TimetableSession> = {}): TimetableSession {
  const startMs = atToday(8, 30);
  return {
    date: localDateKey(new Date()),
    courseName: "高等数学",
    buildingName: "致理楼",
    classroom: "301",
    buildingNameEn: null,
    startMs,
    endMs: startMs + 40 * 60_000,
    semester: "2026 秋",
    checkin: "pending_lms",
    ...overrides,
  };
}

describe("WeekGrid", () => {
  beforeEach(() => {
    clock.now = atToday(8, 0);
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("lays sessions out on a weekday × period grid with a course colour", () => {
    clock.now = atToday(8, 45);
    render(<WeekGrid sessions={[session()]} semesterCourses={[]} />);
    expect(screen.getByText("周一")).toBeInTheDocument();
    expect(screen.getByText("周日")).toBeInTheDocument();
    expect(screen.getByText("第1节")).toBeInTheDocument();
    const card = screen.getByRole("button", { name: /高等数学/ });
    expect(card.className).toMatch(/bg-accent-\d+-soft/);
    expect(screen.getByText("上课中")).toBeInTheDocument();
    expect(screen.getByText("高等数学").className).toContain("font-semibold");
    expect(document.querySelector("table")).toBeNull();
  });

  it("renders finished courses as a ghost and keeps the check-in label", () => {
    clock.now = atToday(10, 0);
    render(<WeekGrid sessions={[session()]} semesterCourses={[]} />);
    const card = screen.getByRole("button", { name: /高等数学/ });
    expect(card.className).toContain("border-dashed");
    expect(card.className).toContain("opacity-55");
    expect(screen.getByText("待签·畅课")).toBeInTheDocument();
  });

  it("opens the detail dialog and offers the matching sign-in page", () => {
    render(<WeekGrid sessions={[session()]} semesterCourses={[]} />);
    fireEvent.click(screen.getByRole("button", { name: /高等数学/ }));
    expect(document.querySelector("m-dialog")).not.toBeNull();
    expect(screen.getByText("待签到(畅课lms)")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "去签到" }));
    expect(nav.push).toHaveBeenCalledWith(
      expect.stringMatching(/^\/study\/signin\?tab=lms&course=.*&open=1$/),
    );
  });

  it("keeps sessions that do not match a period in a separate section", () => {
    render(
      <WeekGrid
        sessions={[session({ startMs: atToday(7, 0), endMs: atToday(7, 0) + 30 * 60_000 })]}
        semesterCourses={[]}
      />,
    );
    expect(screen.getAllByText("其他时段").length).toBeGreaterThan(0);
  });
});
