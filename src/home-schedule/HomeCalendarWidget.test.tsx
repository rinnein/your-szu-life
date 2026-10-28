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
  });

  beforeEach(() => {
    vi.spyOn(navigationModule, "useLogicalNavigation").mockReturnValue({
      push: pushMock,
      replace: vi.fn(),
      back: vi.fn(),
      canBack: () => false,
      lastAction: () => null,
    });
  });

  it("renders calendar month and list of deadlines", () => {
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

    // Check heading
    expect(screen.getByRole("heading", { name: /日历与待办/ })).toBeInTheDocument();
    expect(screen.getByText("2026年 10月")).toBeInTheDocument();

    // Check todo items rendered
    expect(screen.getByText("算法第四次测验")).toBeInTheDocument();
    expect(screen.getByText("预计电量耗尽（剩余 5 度）")).toBeInTheDocument();

    // Check category chips
    expect(screen.getByText("测验")).toBeInTheDocument();
    expect(screen.getByText("电量")).toBeInTheDocument();
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

  it("navigates to the corresponding feature when clicking a todo item", () => {
    const testDate = new Date(2026, 9, 28);
    const mockItems = [
      {
        id: "oj:1",
        category: "oj-quiz" as const,
        title: "算法第四次测验",
        targetDate: new Date(2026, 9, 28, 23, 59, 0),
        dateKey: "2026-10-28",
        actionUrl: "/study/oj?site=oj1&contest=1",
      },
    ];

    vi.spyOn(deadlinesModule, "useHomeDeadlines").mockReturnValue({
      items: mockItems,
      categoryMap: new Map([["2026-10-28", ["oj-quiz"]]]),
      loading: false,
      refresh: vi.fn(),
    });

    render(<HomeCalendarWidget initialDate={testDate} />);

    const itemBtn = screen.getByRole("button", { name: /算法第四次测验/ });
    fireEvent.click(itemBtn);

    expect(pushMock).toHaveBeenCalledWith("/study/oj?site=oj1&contest=1");
  });
});
