/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { HomeworkListPanel } from "./HomeworkListPanel";
import type { HomeworkTask, UoocCourse } from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const mockCourses: UoocCourse[] = [
  {
    id: "101",
    title: "计算机网络",
    semester: "2026秋",
    organization: "深圳大学",
    weeks: "1-16周",
  },
];

const mockHomeworks: HomeworkTask[] = [
  {
    id: "h1",
    title: "第一章课后作业",
    cid: "101",
    courseName: "计算机网络",
    startTime: "2026-10-01 00:00:00",
    endTime: "2026-10-28 23:59:59",
    status: 1,
    chance: 3,
    submitCount: 1,
    score: null,
    taskId: "t2001",
    hasEval: true,
  },
  {
    id: "h2",
    title: "第二章课后作业",
    cid: "101",
    courseName: "计算机网络",
    startTime: "2026-09-01 00:00:00",
    endTime: "2026-09-20 23:59:59",
    status: 2,
    chance: 1,
    submitCount: 1,
    score: 98,
    taskId: "t2002",
    hasEval: false,
  },
];

describe("HomeworkListPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it("fetches and renders homework tasks correctly", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === "list_uooc_homeworks") {
        return mockHomeworks;
      }
      return null;
    });

    render(<HomeworkListPanel courses={mockCourses} ready={true} />);

    await waitFor(() => {
      expect(screen.getByText("第一章课后作业")).toBeInTheDocument();
      expect(screen.getByText("第二章课后作业")).toBeInTheDocument();
    });

    expect(screen.getByText("含互评环节")).toBeInTheDocument();
    expect(screen.getByText("已批改: 98分")).toBeInTheDocument();
  });

  it("opens peer eval modal when clicking eval button", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === "list_uooc_homeworks") {
        return mockHomeworks;
      }
      if (cmd === "get_uooc_eval_list") {
        return [];
      }
      return null;
    });

    render(<HomeworkListPanel courses={mockCourses} ready={true} />);

    await waitFor(() => {
      expect(screen.getByText("第一章课后作业")).toBeInTheDocument();
    });

    const evalButtons = screen.getAllByRole("button", { name: /互评/ });
    expect(evalButtons.length).toBeGreaterThan(0);

    await act(async () => {
      fireEvent.click(evalButtons[0]);
    });

    await waitFor(() => {
      expect(screen.getByText("课程作业互评助手")).toBeInTheDocument();
    });
  });
});
