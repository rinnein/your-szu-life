/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { CourseProgressView } from "./CourseProgressView";
import { courseSchedule } from "./course-schedule";
import type { ProgressEntry, UoocCourse } from "./types";

Element.prototype.getAnimations = () => [];
beforeEach(() =>
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  ),
);
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it.each([
  ["0/10", "accent"],
  ["4/10", "accent"],
  ["5/10", "success"],
  ["6/10", "success"],
  ["4/6", "warning"],
  ["7/10", "warning"],
  ["8/10", "warning"],
  ["6/7", "danger"],
  ["9/10", "danger"],
  ["10/10", "danger"],
  ["12/10", "danger"],
  ["1/2", "danger"],
  ["1/1", "danger"],
])("uses the configured week thresholds for %s", (weeks, color) => {
  expect(courseSchedule(weeks)?.color).toBe(color);
});

it("accepts whitespace, caps elapsed courses, and rejects unknown week values", () => {
  expect(courseSchedule(" 5 / 10 ")?.percent).toBe(50);
  expect(courseSchedule("12/10")?.percent).toBe(100);
  for (const weeks of ["", "未知", "1/0", "-1/10", "2.5/10", "Infinity/10", "1/9007199254740992"])
    expect(courseSchedule(weeks)).toBeNull();
});

const course: UoocCourse = {
  id: "42",
  title: "测试课程",
  organization: "深圳大学",
  semester: "秋季",
  weeks: "6/7",
  endDate: "2026/10/14",
};
const metric = { percent: null, count: null, total: null };
const entry: ProgressEntry = {
  data: {
    totalPercent: 35,
    video: metric,
    test: metric,
    homework: metric,
    signin: metric,
    signinWeight: null,
    signinTime: null,
    attendance: "unknown",
  },
  updatedAtMs: null,
  refreshing: false,
  error: null,
};

it("shows weeks and the end date above the independent server course progress", () => {
  render(<CourseProgressView course={course} entry={entry} />);
  expect(screen.getByText("周程进度 · 6/7")).toBeInTheDocument();
  expect(screen.getByText("结课日期：2026/10/14")).toBeInTheDocument();
  const bars = screen.getAllByRole("progressbar");
  expect(bars.map((bar) => bar.getAttribute("aria-label"))).toEqual(["周程进度", "课程总进度"]);
  // M3 用 data 属性表达配色，而不是 BEM 变体类。
  expect(bars[0]).toHaveAttribute("data-color", "danger");
  expect(Number(bars[0].getAttribute("aria-valuenow"))).toBeCloseTo(600 / 7);
  expect(bars[1]).toHaveAttribute("aria-valuenow", "35");
});

it("keeps legacy courses readable without inventing a date or percentage", () => {
  render(
    <CourseProgressView course={{ ...course, weeks: "未知", endDate: undefined }} entry={entry} />,
  );
  expect(screen.getByText("结课日期：—")).toBeInTheDocument();
  expect(screen.getByText("周程进度未知")).toBeInTheDocument();
  expect(screen.queryByRole("progressbar", { name: "周程进度" })).not.toBeInTheDocument();
  expect(screen.getByRole("progressbar", { name: "课程总进度" })).toHaveAttribute(
    "aria-valuenow",
    "35",
  );
});

it("expands dynamic weights and dims only zero-weight categories", async () => {
  const refresh = vi.fn();
  const data = {
    ...entry.data!,
    totalPercent: 7,
    weightedPercent: 37.5,
    weightedPartial: true,
    weightTotal: 100,
    score: 25.5,
    weightedScore: 23.5,
    discussionPosts: 1,
    discussionReplies: 2,
    signin: { percent: 50, count: 10, total: 20, weight: 5, score: 2.5 },
    video: { percent: 50, count: 28, total: 57, weight: 30, score: 15 },
    homework: { percent: 0, count: 0, total: 0, weight: 10, score: 0 },
    test: { percent: 100, count: 13, total: 13, weight: 20, score: 20 },
    discussion: { percent: null, count: 3, total: null, weight: 0, score: 0 },
    exam: { percent: null, count: 0, total: 0, weight: 35, score: 0 },
  };
  render(<CourseProgressView course={course} entry={{ ...entry, data }} onRefresh={refresh} />);
  console.log(
    "[W] all:",
    Array.from(document.querySelectorAll("span"))
      .map((e) => e.textContent)
      .filter(Boolean)
      .slice(0, 30),
  );
  expect(screen.getByRole("progressbar", { name: "课程总进度" })).toHaveAttribute(
    "aria-valuenow",
    "37.5",
  );
  expect(screen.queryByRole("grid")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "测试课程的课程详情" }));
  const table = await screen.findByRole("grid", { name: "测试课程的考核权重与任务" });
  expect(refresh).toHaveBeenCalledTimes(1);
  const rows = within(table).getAllByRole("row").slice(1);
  expect(rows.map((row) => within(row).getByRole("rowheader").textContent)).toEqual([
    "签到",
    "视频",
    "作业",
    "测验",
    "讨论",
    "考试",
  ]);
  expect(rows[1]).toHaveTextContent("30%");
  expect(rows[2]).toHaveTextContent("暂无任务");
  expect(rows[2]).not.toHaveClass("opacity-50");
  expect(rows[4]).toHaveClass("opacity-50");
  expect(rows[4]).toHaveTextContent("不计入进度");
  expect(rows[5]).toHaveTextContent("35%");
  expect(screen.getByText("平台总进度：7%")).toBeInTheDocument();
  expect(screen.getByText("加权成绩：23.5")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "测试课程的课程详情" }));
  expect(refresh).toHaveBeenCalledTimes(1);
});
