/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vite-plus/test";
import { TermGrid } from "./TermGrid";
import type { SemesterCourse } from "./types";

const courses: SemesterCourse[] = [
  {
    name: "编译原理",
    teacher: "演示教师",
    classroom: "demo-room-1",
    weekday: 1,
    startPeriod: 1,
    endPeriod: 2,
    weeks: [1, 2, 3, 4],
    rawWeeks: "1-4周",
  },
];

describe("TermGrid", () => {
  afterEach(cleanup);

  it("renders the eHall fixed timetable as a weekday grid", () => {
    render(<TermGrid courses={courses} message="" fetchedAtMs={Date.now()} />);
    expect(screen.getByText("编译原理")).toBeInTheDocument();
    expect(screen.getByText("周一")).toBeInTheDocument();
    expect(screen.getByText("周日")).toBeInTheDocument();
    expect(screen.getByText("第1节")).toBeInTheDocument();
    expect(screen.getByText("1-4 周")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /编译原理/ }));
    expect(document.querySelector("m-dialog")).not.toBeNull();
    expect(screen.getByText(/第1–2节/)).toBeInTheDocument();
  });

  it("explains the empty cache without adding its own action buttons", () => {
    render(<TermGrid courses={[]} message="请先完成教务官方认证" fetchedAtMs={0} />);
    expect(screen.getByText("请先完成教务官方认证")).toBeInTheDocument();
    expect(screen.queryByRole("button")).toBeNull();
  });
});
