/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { QuizListPanel } from "./QuizListPanel";
import type { HomeworkTask, QuizState, UoocCourse } from "./types";

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

const mockQuizTasks: HomeworkTask[] = [
  {
    id: "q1",
    title: "1.1 互联网概述随堂测验",
    cid: "101",
    courseName: "计算机网络",
    startTime: "2019-12-24 16:13:09",
    endTime: "2020-12-23 16:13:09", // Nominal past end_time from UOOC
    status: 0,
    statusText: "未提交",
    statusCode: 0,
    canDo: true,
    canView: false,
    canEval: false,
    chance: 0,
    submitCount: 0,
    score: null,
    taskId: "t101",
    hasEval: false,
    chapterText: "第1章 走进网络",
    sectionText: "第1节 互联网概述",
  },
];

const mockQuizzes: Record<string, QuizState> = {
  q1: {
    accountId: "u1",
    cid: "101",
    title: "1.1 互联网概述随堂测验",
    taskId: "t101",
    resourceId: "r1",
    stage: "awaitingApproval",
    mode: "supervised",
    answers: {},
    questions: [
      {
        id: "q_1",
        text: "什么是包交换？",
        kind: "single",
        options: [{ id: "A", text: "数据分组存储转发" }],
        blanks: 0,
        images: [],
      },
    ],
    reviews: [],
    revision: 1,
    message: "待确认提交",
    gated: false,
    videoComplete: true,
    passed: [],
  },
};

describe("QuizListPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it("does not rely on end_time and displays can_do quiz as enterable", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === "list_uooc_quizzes") {
        return mockQuizTasks;
      }
      return null;
    });

    const handleOpen = vi.fn();
    render(
      <QuizListPanel
        courses={mockCourses}
        quizzes={mockQuizzes}
        onOpenQuiz={handleOpen}
        ready={true}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText("1.1 互联网概述随堂测验")).toBeInTheDocument();
      expect(screen.getByText("第1章 走进网络 · 第1节 互联网概述")).toBeInTheDocument();
    });

    // 即使 end_time 在过去，由于 canDo: true，依然判定为可作答/未提交，而不是已截止
    expect(screen.getByText("未提交")).toBeInTheDocument();
    expect(screen.queryByText("已截止")).not.toBeInTheDocument();

    const openBtn = screen.getByRole("button", { name: /进入测验/ });
    await act(async () => {
      fireEvent.click(openBtn);
    });

    expect(handleOpen).toHaveBeenCalledWith(mockQuizzes.q1);
  });

  it("fetches completed quiz via view api when clicking completed quiz", async () => {
    const completedTasks: HomeworkTask[] = [
      {
        ...mockQuizTasks[0],
        status: 20,
        statusText: "已批改",
        statusCode: 20,
        canDo: false,
        canView: true,
        submitCount: 1,
        score: 100,
      },
    ];

    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === "list_uooc_quizzes") {
        return completedTasks;
      }
      if (cmd === "fetch_uooc_completed_quiz") {
        return {
          quizzes: {
            q1: {
              ...mockQuizzes.q1,
              stage: "passed",
            },
          },
        };
      }
      return null;
    });

    const handleOpen = vi.fn();
    const handleSnapshot = vi.fn();
    render(
      <QuizListPanel
        courses={mockCourses}
        quizzes={mockQuizzes}
        onOpenQuiz={handleOpen}
        onSnapshot={handleSnapshot}
        ready={true}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText("已批改: 100分")).toBeInTheDocument();
    });

    const viewBtn = screen.getByRole("button", { name: /查看答卷/ });
    await act(async () => {
      fireEvent.click(viewBtn);
    });

    expect(invoke).toHaveBeenCalledWith("fetch_uooc_completed_quiz", {
      cid: "101",
      taskId: "t101",
    });
    expect(handleSnapshot).toHaveBeenCalled();
  });
});
