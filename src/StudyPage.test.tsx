/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import StudyPage from "./StudyPage";

const nav = vi.hoisted(() => ({ push: vi.fn(), replace: vi.fn(), back: vi.fn() }));
vi.mock("./logical-navigation", () => ({ useLogicalNavigation: () => nav }));
vi.mock("./auth", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./auth")>();
  return {
    ...actual,
    useAuth: () => ({ state: { Authenticated: { studentId: "123456", name: "测试同学" } } }),
  };
});

describe("StudyPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });
  afterEach(cleanup);

  it("直接呈现 OJ 功能卡片且无计软合集卡片，点击导航到 /study/oj", () => {
    render(<StudyPage />);
    expect(screen.queryByText("计软合集")).toBeNull();
    const ojButton = screen.getByRole("button", { name: /OJ.*班级、测验与编程练习/ });
    expect(ojButton).toBeInTheDocument();
    fireEvent.click(ojButton);
    expect(nav.push).toHaveBeenCalledWith("/study/oj");
  });

  it("点击成绩查询导航到 /study/grades", () => {
    render(<StudyPage />);
    const gradesButton = screen.getByRole("button", { name: /成绩查询.*各学期课程成绩/ });
    expect(gradesButton).toBeInTheDocument();
    fireEvent.click(gradesButton);
    expect(nav.push).toHaveBeenCalledWith("/study/grades");
  });

  it("点击教学评价导航到 /study/evaluation", () => {
    render(<StudyPage />);
    const evaluationButton = screen.getByRole("button", { name: /教学评价.*学生评教/ });
    expect(evaluationButton).toBeInTheDocument();
    fireEvent.click(evaluationButton);
    expect(nav.push).toHaveBeenCalledWith("/study/evaluation");
  });

  it("按日常/周期分组，各自只包含本组入口", () => {
    render(<StudyPage />);
    const daily = screen.getByRole("region", { name: "日常" });
    const periodic = screen.getByRole("region", { name: "周期" });

    for (const label of ["课表", "签到/打卡", "畅课 lms", "UOOC刷课", "OJ", "监察巡课"]) {
      expect(
        within(daily).getByRole("button", { name: new RegExp(`^${label}`) }),
      ).toBeInTheDocument();
    }
    expect(within(daily).getAllByRole("button")).toHaveLength(6);
    // 组内按常用度排列：课表在最前，监察巡课在最后。
    const dailyButtons = within(daily).getAllByRole("button");
    expect(dailyButtons[0]).toHaveTextContent("课表");
    expect(dailyButtons[5]).toHaveTextContent("监察巡课");

    for (const label of ["选课", "学业完成查询", "成绩查询", "教学评价"]) {
      expect(
        within(periodic).getByRole("button", { name: new RegExp(`^${label}`) }),
      ).toBeInTheDocument();
    }
    expect(within(periodic).getAllByRole("button")).toHaveLength(4);
  });

  it("两组之间只有一条分隔线，且落在日常组与周期组之间", () => {
    const { container } = render(<StudyPage />);
    const dividers = container.querySelectorAll(".feature-groups-divider");
    expect(dividers).toHaveLength(1);

    const daily = screen.getByRole("region", { name: "日常" });
    const periodic = screen.getByRole("region", { name: "周期" });
    const divider = dividers[0];

    expect(daily.compareDocumentPosition(divider) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(
      divider.compareDocumentPosition(periodic) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });
});
