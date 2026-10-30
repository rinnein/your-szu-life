/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { HomeCalendarWidget } from "./HomeCalendarWidget";
import * as deadlinesModule from "./use-home-deadlines";
import * as navigationModule from "../logical-navigation";

describe("HomeCalendarWidget", () => {
  const pushMock = vi.fn();

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
    vi.useRealTimers();
  });

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2026, 9, 28, 12, 0, 0));
    vi.spyOn(navigationModule, "useLogicalNavigation").mockReturnValue({
      push: pushMock,
      replace: vi.fn(),
      back: vi.fn(),
      canBack: () => false,
      lastAction: () => null,
    });
  });

  it("renders calendar month and list of deadlines with UOOC items", () => {
    const testDate = new Date(2026, 9, 28, 12, 0, 0); // 2026-10-28
    const mockItems = [
      {
        id: "oj:1",
        category: "oj-quiz" as const,
        title: "算法第四次测验",
        subtitle: "计算机学院OJ",
        targetDate: new Date(2026, 9, 28, 23, 59, 0),
        dateKey: "2026-10-28",
        actionUrl: "/study/oj?site=oj1&contest=1",
      },
      {
        id: "elec:1",
        category: "electricity-outage" as const,
        title: "预计电量耗尽（剩余 5 度）",
        subtitle: "宿舍电费",
        targetDate: new Date(2026, 9, 31, 12, 0, 0),
        dateKey: "2026-10-31",
        actionUrl: "/life/campus-payments/recharge",
      },
      {
        id: "uooc:exam:1",
        category: "uooc-exam" as const,
        title: "计算机网络期末考试",
        subtitle: "计算机网络 · 考试",
        targetDate: new Date(2026, 9, 29, 20, 0, 0),
        dateKey: "2026-10-29",
        actionUrl: "/study/uooc?tab=exams&subtab=exam",
      },
      {
        id: "uooc:hw:1",
        category: "uooc-homework" as const,
        title: "操作系统第四章作业",
        subtitle: "操作系统 · 作业",
        targetDate: new Date(2026, 9, 30, 23, 59, 0),
        dateKey: "2026-10-30",
        actionUrl: "/study/uooc?tab=exams&subtab=homework",
      },
      {
        id: "uooc:course:1",
        category: "uooc-course-end" as const,
        title: "大学英语 课程结课",
        subtitle: "UOOC · 课程结课",
        targetDate: new Date(2026, 10, 5, 23, 59, 0),
        dateKey: "2026-11-05",
        actionUrl: "/study/uooc?tab=courses",
      },
    ];

    vi.spyOn(deadlinesModule, "useHomeDeadlines").mockReturnValue({
      items: mockItems,
      categoryMap: new Map([
        ["2026-10-28", ["oj-quiz"]],
        ["2026-10-29", ["uooc-exam"]],
        ["2026-10-30", ["uooc-homework"]],
        ["2026-10-31", ["electricity-outage"]],
        ["2026-11-05", ["uooc-course-end"]],
      ]),
      loading: false,
      refresh: vi.fn(),
    });

    render(<HomeCalendarWidget initialDate={testDate} />);

    // Check heading
    expect(screen.getByRole("heading", { name: /日历与待办/ })).toBeInTheDocument();
    expect(screen.getByText("2026年 10月")).toBeInTheDocument();

    // Check todo items rendered
    expect(screen.getByText("算法第四次测验")).toBeInTheDocument();
    expect(screen.getByText("预计电量耗尽（剩余 5 度）")).toBeInTheDocument();
    expect(screen.getByText("计算机网络期末考试")).toBeInTheDocument();
    expect(screen.getByText("操作系统第四章作业")).toBeInTheDocument();
    expect(screen.getByText("大学英语 课程结课")).toBeInTheDocument();

    // Check category chips
    expect(screen.getByText("OJ测验")).toBeInTheDocument();
    expect(screen.getByText("电量")).toBeInTheDocument();
    expect(screen.getByText("UOOC考试")).toBeInTheDocument();
    expect(screen.getByText("UOOC作业")).toBeInTheDocument();
    expect(screen.getByText("UOOC结课")).toBeInTheDocument();

    // Check legend items
    expect(screen.getByText("UOOC 考试")).toBeInTheDocument();
    expect(screen.getByText("UOOC 作业")).toBeInTheDocument();
    expect(screen.getByText("UOOC 结课")).toBeInTheDocument();

    // 电量预警必须落到主题里真实存在的 danger 色，而不是未定义的 bg-error。
    const day31 = screen.getByRole("button", { name: "2026年10月31日" });
    expect(day31.querySelector(".bg-danger")).not.toBeNull();
    expect(screen.getByText("电量预警").previousElementSibling).toHaveClass("bg-danger");
  });

  it("filters todo list when clicking on a date with events and clears filter on click again", () => {
    const testDate = new Date(2026, 9, 28, 12, 0, 0);
    const mockItems = [
      {
        id: "oj:1",
        category: "oj-quiz" as const,
        title: "算法第四次测验",
        targetDate: new Date(2026, 9, 28, 23, 59, 0),
        dateKey: "2026-10-28",
        actionUrl: "/study/oj?site=oj1&contest=1",
      },
      {
        id: "elec:1",
        category: "electricity-outage" as const,
        title: "预计电量耗尽",
        targetDate: new Date(2026, 9, 31, 12, 0, 0),
        dateKey: "2026-10-31",
        actionUrl: "/life/campus-payments/recharge",
      },
    ];

    vi.spyOn(deadlinesModule, "useHomeDeadlines").mockReturnValue({
      items: mockItems,
      categoryMap: new Map([
        ["2026-10-28", ["oj-quiz"]],
        ["2026-10-31", ["electricity-outage"]],
      ]),
      loading: false,
      refresh: vi.fn(),
    });

    render(<HomeCalendarWidget initialDate={testDate} />);

    // Initially both items are shown
    expect(screen.getByText("算法第四次测验")).toBeInTheDocument();
    expect(screen.getByText("预计电量耗尽")).toBeInTheDocument();

    // Click 28th day button
    const day28Btn = screen.getByRole("button", { name: "2026年10月28日" });
    fireEvent.click(day28Btn);

    // Now filtered to 10-28
    expect(screen.getByText("算法第四次测验")).toBeInTheDocument();
    expect(screen.queryByText("预计电量耗尽")).not.toBeInTheDocument();
    expect(screen.getByText(/10月28日/)).toBeInTheDocument();

    // Click "全部待办" or clear button to reset
    const clearBtn = screen.getByRole("button", { name: "全部待办" });
    fireEvent.click(clearBtn);

    // Both are visible again
    expect(screen.getByText("算法第四次测验")).toBeInTheDocument();
    expect(screen.getByText("预计电量耗尽")).toBeInTheDocument();
  });

  it("navigates to the corresponding feature when clicking a UOOC exam or homework item", () => {
    const testDate = new Date(2026, 9, 28);
    const mockItems = [
      {
        id: "uooc:exam:1",
        category: "uooc-exam" as const,
        title: "期中考试",
        targetDate: new Date(2026, 9, 29, 20, 0, 0),
        dateKey: "2026-10-29",
        actionUrl: "/study/uooc?tab=exams&subtab=exam",
      },
    ];

    vi.spyOn(deadlinesModule, "useHomeDeadlines").mockReturnValue({
      items: mockItems,
      categoryMap: new Map([["2026-10-29", ["uooc-exam"]]]),
      loading: false,
      refresh: vi.fn(),
    });

    render(<HomeCalendarWidget initialDate={testDate} />);

    const itemBtn = screen.getByRole("button", { name: /期中考试/ });
    fireEvent.click(itemBtn);

    expect(pushMock).toHaveBeenCalledWith("/study/uooc?tab=exams&subtab=exam");
  });
});
