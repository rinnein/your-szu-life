/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import GradesPage from "./GradesPage";
import { mockEnrollmentGrades } from "../demo/fixtures/enrollment";
import type { GradeReport } from "./model";

const enrollment = vi.hoisted(() => ({
  snapshot: null as unknown,
  pending: {} as Record<string, number>,
  errors: {} as Record<string, string>,
  run: vi.fn(),
}));

vi.mock("./use-enrollment", () => ({
  useEnrollment: () => ({
    snapshot: enrollment.snapshot,
    pending: enrollment.pending,
    errors: enrollment.errors,
    run: enrollment.run,
  }),
}));

vi.mock("../logical-navigation", () => ({
  nativeRuntime: () => false,
  isChildDocument: () => false,
  useLogicalNavigation: () => ({
    push: vi.fn(),
    replace: vi.fn(),
    back: vi.fn(),
    canBack: true,
  }),
}));

const snapshotWith = (grades: GradeReport | null) => ({
  context: { student: "20250001", term: "2026-2027-1", batch: "", campus: "" },
  grades,
});

describe("GradesPage", () => {
  beforeEach(() => {
    enrollment.snapshot = snapshotWith(mockEnrollmentGrades);
    enrollment.pending = {};
    enrollment.errors = {};
    enrollment.run.mockReset();
  });
  afterEach(cleanup);

  it("默认显示最新学期与总 GPA", () => {
    render(<GradesPage />);

    expect(screen.getByText("总 GPA")).toBeInTheDocument();
    expect(screen.getByText("3.75")).toBeInTheDocument();

    const grid = screen.getByRole("grid", { name: /2026-2027学年第一学期成绩/ });
    expect(within(grid).getByText("编译原理")).toBeInTheDocument();
    expect(within(grid).getByText("大学英语（1）")).toBeInTheDocument();
    expect(within(grid).queryByText("数据结构")).toBeNull();
  });

  it("切换学期后显示该学期课程", () => {
    const { container } = render(<GradesPage />);
    const select = container.querySelector("m-select");
    expect(select).not.toBeNull();
    fireEvent(select!, new CustomEvent("change", { detail: { value: "2025-2026-2" } }));

    const grid = screen.getByRole("grid", { name: /2025-2026学年第二学期成绩/ });
    expect(within(grid).getByText("数据结构")).toBeInTheDocument();
    expect(within(grid).getAllByText("无效").length).toBeGreaterThan(0);
    expect(within(grid).queryByText("编译原理")).toBeNull();
  });

  it("刷新会重新拉取成绩", () => {
    render(<GradesPage />);
    fireEvent.click(screen.getByRole("button", { name: "刷新" }));
    expect(enrollment.run).toHaveBeenCalledWith({ type: "grades" });
  });

  it("没有缓存时自动查询一次", async () => {
    enrollment.snapshot = snapshotWith(null);
    render(<GradesPage />);
    await waitFor(() => expect(enrollment.run).toHaveBeenCalledWith({ type: "grades" }));
    expect(enrollment.run).toHaveBeenCalledTimes(1);
  });

  it("加载中显示骨架而不显示空态", () => {
    enrollment.snapshot = snapshotWith(null);
    enrollment.pending = { grades: 1 };
    render(<GradesPage />);
    expect(screen.getByRole("status", { name: "正在加载成绩" })).toBeInTheDocument();
    expect(screen.queryByText("还没有成绩数据")).toBeNull();
    expect(enrollment.run).not.toHaveBeenCalled();
  });
});
