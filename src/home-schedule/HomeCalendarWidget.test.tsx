/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { HomeCalendarWidget } from "./HomeCalendarWidget";
import * as navigationModule from "../logical-navigation";
import type { ScheduleCategory, ScheduleItem } from "./types";

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
      pin: vi.fn(),
      replace: vi.fn(),
      back: vi.fn(),
      canBack: () => false,
      lastAction: () => null,
    });
  });

  function renderWidget(
    items: ScheduleItem[],
    categoryMap: Map<string, ScheduleCategory[]>,
    initialDate: Date,
  ) {
    return render(
      <HomeCalendarWidget
        items={items}
        categoryMap={categoryMap}
        loading={false}
        ready
        refresh={vi.fn()}
        initialDate={initialDate}
      />,
    );
  }

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

    renderWidget(
      mockItems,
      new Map([
        ["2026-10-28", ["oj-quiz"]],
        ["2026-10-29", ["uooc-exam"]],
        ["2026-10-30", ["uooc-homework"]],
        ["2026-10-31", ["electricity-outage"]],
        ["2026-11-05", ["uooc-course-end"]],
      ]),
      testDate,
    );

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

    // 待办类别已统一使用动态分类色板；日历圆点与图例必须保持同一角色。
    const day31 = screen.getByRole("button", { name: "2026年10月31日" });
    expect(day31.querySelector(".bg-accent-2")).not.toBeNull();
    expect(screen.getByText("电量预警").previousElementSibling).toHaveClass("bg-accent-2");
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

    renderWidget(
      mockItems,
      new Map([
        ["2026-10-28", ["oj-quiz"]],
        ["2026-10-31", ["electricity-outage"]],
      ]),
      testDate,
    );

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

    renderWidget(mockItems, new Map([["2026-10-29", ["uooc-exam"]]]), testDate);

    const itemBtn = screen.getByRole("button", { name: /期中考试/ });
    fireEvent.click(itemBtn);

    expect(pushMock).toHaveBeenCalledWith("/study/uooc?tab=exams&subtab=exam");
  });

  it("shows a skeleton instead of the empty state before todos arrive", () => {
    render(
      <HomeCalendarWidget
        items={[]}
        categoryMap={new Map()}
        loading
        ready={false}
        refresh={vi.fn()}
        initialDate={new Date(2026, 9, 28, 12, 0, 0)}
      />,
    );

    expect(screen.getByRole("status", { name: "正在读取待办" })).toBeInTheDocument();
    expect(screen.queryByText("近期暂无待办事项，一切就绪")).not.toBeInTheDocument();
  });

  it("colors the evaluation todo with the seventh palette slot and links to the page", () => {
    const testDate = new Date(2026, 9, 28, 12, 0, 0);
    const item: ScheduleItem = {
      id: "evaluation:pending",
      category: "evaluation-pending",
      title: "评教未完成",
      subtitle: "点击一键完成 · 2 位教师待评",
      targetDate: new Date(2026, 9, 28, 23, 59, 0),
      dateKey: "2026-10-28",
      actionUrl: "/study/evaluation",
      urgent: true,
    };
    renderWidget([item], new Map([["2026-10-28", ["evaluation-pending"]]]), testDate);

    // 待办列表 chip 与图例使用同一档 accent-7；日历圆点同样如此。
    expect(screen.getAllByText("评教").length).toBeGreaterThanOrEqual(2);
    const legend = screen.getByText("评教", { selector: ".flex.items-center > span:last-child" });
    expect(legend.previousElementSibling).toHaveClass("bg-accent-7");
    const day28 = screen.getByRole("button", { name: "2026年10月28日" });
    expect(day28.querySelector(".bg-accent-7")).not.toBeNull();

    fireEvent.click(screen.getByRole("button", { name: /评教：评教未完成/ }));
    expect(pushMock).toHaveBeenCalledWith("/study/evaluation");
  });
});
