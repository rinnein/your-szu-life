/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { describe, expect, it, vi, beforeEach, afterEach } from "vite-plus/test";
import { render, screen, fireEvent, waitFor, act, cleanup } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { ExamPaperModal } from "./ExamPaperModal";
import type { ExamPaper, ExamTask } from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));

afterEach(() => {
  cleanup();
  localStorage.clear();
});

const mockExam: ExamTask = {
  id: "e101",
  title: "计算机网络期末考试",
  cid: "c101",
  courseName: "计算机网络",
  startTime: "2026-10-01 09:00:00",
  endTime: "2026-10-31 18:00:00",
  limitTime: 90,
  status: 1,
  chance: 1,
  submitCount: 0,
  taskId: "t101",
};

const mockPaper: ExamPaper = {
  taskId: "t101",
  title: "计算机网络期末考试",
  cid: "c101",
  limitTime: 90,
  questions: [
    {
      id: "q1",
      text: "TCP 协议建立连接需要几次握手？",
      kind: "single",
      options: [
        { id: "A", text: "1次" },
        { id: "B", text: "2次" },
        { id: "C", text: "3次" },
        { id: "D", text: "4次" },
      ],
      blanks: 0,
      images: [],
    },
    {
      id: "q2",
      text: "下列属于传输层协议的有：",
      kind: "multiple",
      options: [
        { id: "A", text: "TCP" },
        { id: "B", text: "UDP" },
        { id: "C", text: "IP" },
      ],
      blanks: 0,
      images: [],
    },
    {
      id: "q3",
      text: "请简述三次握手的主要作用。",
      kind: "subjective",
      options: [],
      blanks: 0,
      images: [],
    },
  ],
  answers: {},
  reviews: [],
  isSubmitted: false,
};

describe("ExamPaperModal", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    Element.prototype.getAnimations = () => [];
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_uooc_exam_paper") {
        return Promise.resolve(mockPaper);
      }
      if (cmd === "save_uooc_exam_draft") {
        return Promise.resolve();
      }
      if (cmd === "submit_uooc_exam") {
        return Promise.resolve("试卷已成功提交");
      }
      if (cmd === "match_uooc_paper_qa") {
        return Promise.resolve([
          { q1: ["C"] },
          [
            {
              questionId: "q1",
              answers: ["C"],
              probabilities: {},
              message: "题库单题精确命中 (标准答案)",
            },
          ],
        ]);
      }
      if (cmd === "solve_uooc_paper_ai") {
        return Promise.resolve([
          { q2: ["A", "B"], q3: ["同步双方初始序列号，确认双方发送与接收正常"] },
          [
            {
              questionId: "q2",
              answers: ["A", "B"],
              probabilities: { A: 0.95, B: 0.95, C: 0.05 },
              message: "AI 判定建议选项: A, B",
            },
            {
              questionId: "q3",
              answers: ["同步双方初始序列号，确认双方发送与接收正常"],
              probabilities: {},
              message: "AI 理由: 建立可靠通信连接",
            },
          ],
        ]);
      }
      if (cmd === "generate_uooc_subjective") {
        return Promise.resolve("AI 生成简答草稿：三次握手确保全双工通道正常打开。");
      }
      return Promise.resolve();
    });
  });

  it("loads paper and displays questions with countdown and tags", async () => {
    render(<ExamPaperModal exam={mockExam} isOpen={true} onClose={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("计算机网络期末考试")).toBeInTheDocument();
      expect(screen.getByText("TCP 协议建立连接需要几次握手？")).toBeInTheDocument();
      expect(screen.getByText("下列属于传输层协议的有：")).toBeInTheDocument();
      expect(screen.getByText("请简述三次握手的主要作用。")).toBeInTheDocument();
      expect(screen.getByText(/剩余时间: 90:00/)).toBeInTheDocument();
    });
  });

  it("executes matchQa with step session modal and updates standard answers", async () => {
    render(<ExamPaperModal exam={mockExam} isOpen={true} onClose={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("TCP 协议建立连接需要几次握手？")).toBeInTheDocument();
    });

    const matchBtn = screen.getByRole("button", { name: /匹配题库/ });
    await act(async () => {
      fireEvent.click(matchBtn);
    });

    // 验证弹出多步骤流程 Modal
    await waitFor(() => {
      expect(screen.getByText("题库匹配流程")).toBeInTheDocument();
      expect(screen.getByText("已提取试卷结构（共 3 道题目）")).toBeInTheDocument();
    });

    // 完成后点击确认并返回作答
    const confirmDoneBtn = screen.getByRole("button", { name: "确认并返回作答" });
    await act(async () => {
      fireEvent.click(confirmDoneBtn);
    });

    await waitFor(() => {
      expect(screen.queryByText("题库匹配流程")).not.toBeInTheDocument();
      expect(screen.getByText("题库单题精确命中 (标准答案)")).toBeInTheDocument();
    });
  });

  it("filters bank confirmed questions before requesting AI and merges answers safely", async () => {
    render(<ExamPaperModal exam={mockExam} isOpen={true} onClose={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("TCP 协议建立连接需要几次握手？")).toBeInTheDocument();
    });

    // 1. 先匹配题库命中 q1
    const matchBtn = screen.getByRole("button", { name: /匹配题库/ });
    await act(async () => {
      fireEvent.click(matchBtn);
    });
    const confirmDoneBtn = await screen.findByRole("button", { name: "确认并返回作答" });
    await act(async () => {
      fireEvent.click(confirmDoneBtn);
    });

    // 2. 再执行 AI 智能答题
    const aiBtn = screen.getByRole("button", { name: /AI 智能答题/ });
    await act(async () => {
      fireEvent.click(aiBtn);
    });

    await waitFor(() => {
      // 验证 solve_uooc_paper_ai 只向 AI 发送了未确认题库标准答案的题目（q2 和 q3，过滤掉了 q1）
      expect(invoke).toHaveBeenCalledWith("solve_uooc_paper_ai", {
        questions: expect.arrayContaining([
          expect.objectContaining({ id: "q2" }),
          expect.objectContaining({ id: "q3" }),
        ]),
      });
      // 检查 q1 不在发送给 AI 的问题列表中
      const calls = vi
        .mocked(invoke)
        .mock.calls.filter((c: unknown[]) => c[0] === "solve_uooc_paper_ai");
      expect(calls.length).toBe(1);
      const sentQuestions = (calls[0][1] as { questions: { id: string }[] }).questions;
      expect(sentQuestions.some((q) => q.id === "q1")).toBe(false);
    });

    const finishAiBtn = await screen.findByRole("button", { name: "确认并返回作答" });
    await act(async () => {
      fireEvent.click(finishAiBtn);
    });

    // 验证 q1 题库标准答案仍然存在，q2/q3 获得了 AI 推荐
    await waitFor(() => {
      expect(screen.getAllByText("题库单题精确命中 (标准答案)").length).toBeGreaterThan(0);
      expect(screen.getAllByText(/AI 理由: 建立可靠通信连接/).length).toBeGreaterThan(0);
    });
  });

  it("handles subjective question AI answer generation", async () => {
    render(<ExamPaperModal exam={mockExam} isOpen={true} onClose={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("请简述三次握手的主要作用。")).toBeInTheDocument();
    });

    const aiSubBtn = screen.getByRole("button", { name: /AI 解答/ });
    await act(async () => {
      fireEvent.click(aiSubBtn);
    });

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("generate_uooc_subjective", {
        stem: "请简述三次握手的主要作用。",
        question: expect.objectContaining({ id: "q3", text: "请简述三次握手的主要作用。" }),
        context: null,
        referenceExamples: null,
      });
      expect(
        screen.getByDisplayValue("AI 生成简答草稿：三次握手确保全双工通道正常打开。"),
      ).toBeInTheDocument();
    });
  });

  it("allows clicking 在官网打开试卷 even when paper is already submitted (passed stage)", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_uooc_exam_paper") {
        return Promise.resolve({ ...mockPaper, isSubmitted: true });
      }
      return Promise.resolve();
    });

    render(<ExamPaperModal exam={mockExam} isOpen={true} onClose={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("已完成")).toBeInTheDocument();
    });

    const portalBtn = screen.getByRole("button", { name: "在官网打开试卷" });
    expect(portalBtn).toBeEnabled();
    await act(async () => {
      fireEvent.click(portalBtn);
    });

    expect(invoke).toHaveBeenCalledWith("open_uooc_task_portal", { taskId: "t101" });
  });

  it("handles 601 course evaluation validation error with guidance alert and portal open", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_uooc_exam_paper") return Promise.resolve(mockPaper);
      if (cmd === "submit_uooc_exam") {
        return Promise.reject(
          "请先完成课程评价：必须先完成课程教学评价方可提交试卷（UOOC 业务码 601）。可在官网打开课程完成评价后再提交。",
        );
      }
      return Promise.resolve();
    });

    render(<ExamPaperModal exam={mockExam} isOpen={true} onClose={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("TCP 协议建立连接需要几次握手？")).toBeInTheDocument();
    });

    // 点击交卷
    const submitBtn = screen.getByRole("button", { name: "提交试卷" });
    await act(async () => {
      fireEvent.click(submitBtn);
    });

    // 确认交卷
    const confirmBtn = await screen.findByRole("button", { name: "确认提交" });
    await act(async () => {
      fireEvent.click(confirmBtn);
    });

    // 验证弹出 601 课程评价警告横幅与前往官网按钮
    await waitFor(() => {
      expect(screen.getByText("提示：需要先完成课程教学评价")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "前往官网完成评价" })).toBeInTheDocument();
    });

    // 点击前往官网完成评价
    const toPortalBtn = screen.getByRole("button", { name: "前往官网完成评价" });
    await act(async () => {
      fireEvent.click(toPortalBtn);
    });

    expect(invoke).toHaveBeenCalledWith("open_uooc_task_portal", { taskId: "t101" });
  });
});
