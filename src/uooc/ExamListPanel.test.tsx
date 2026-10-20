/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { ExamListPanel } from "./ExamListPanel";
import type { ExamPaper, ExamTask, UoocCourse } from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const mockCourses: UoocCourse[] = [
  {
    id: "101",
    title: "计算机网络",
    semester: "2026秋",
    organization: "深圳大学",
    weeks: "1-16周",
  },
  {
    id: "102",
    title: "高等数学",
    semester: "2026秋",
    organization: "深圳大学",
    weeks: "1-16周",
  },
];

const mockExams: ExamTask[] = [
  {
    id: "e1",
    title: "期末模拟考试",
    cid: "101",
    courseName: "计算机网络",
    startTime: "2026-10-01 09:00:00",
    endTime: "2026-10-30 18:00:00",
    limitTime: 90,
    status: 1,
    chance: 1,
    submitCount: 0,
    taskId: "t1001",
  },
  {
    id: "e2",
    title: "第一阶段测试考试",
    cid: "102",
    courseName: "高等数学",
    startTime: "2026-09-01 09:00:00",
    endTime: "2026-09-20 18:00:00",
    limitTime: 60,
    status: 2,
    chance: 1,
    submitCount: 1,
    score: 95,
    taskId: "t1002",
  },
];

const mockPaper: ExamPaper = {
  taskId: "t1001",
  title: "期末模拟考试",
  cid: "101",
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
  ],
  answers: {
    q1: ["C"],
  },
  reviews: [
    {
      questionId: "q1",
      answers: ["C"],
      probabilities: {},
      message: "题库命中 (100%)",
    },
  ],
  isSubmitted: false,
};

beforeEach(() => {
  vi.clearAllMocks();
  Element.prototype.getAnimations = () => [];
  vi.mocked(invoke).mockImplementation((cmd, args) => {
    if (cmd === "list_uooc_exams") {
      const { cid } = (args || {}) as { cid?: string };
      if (cid && cid !== "all") {
        return Promise.resolve(mockExams.filter((e) => e.cid === cid));
      }
      return Promise.resolve(mockExams);
    }
    if (cmd === "get_uooc_exam_paper") {
      return Promise.resolve(mockPaper);
    }
    if (cmd === "save_uooc_exam_draft") {
      return Promise.resolve();
    }
    if (cmd === "submit_uooc_exam") {
      return Promise.resolve("试卷已成功提交");
    }
    if (cmd === "heartbeat_uooc_exam") {
      return Promise.resolve(true);
    }
    return Promise.reject(new Error(`Unhandled mock command: ${cmd}`));
  });
});

afterEach(() => {
  cleanup();
});

it("renders exam tasks correctly in list panel", async () => {
  render(<ExamListPanel courses={mockCourses} ready={true} />);

  await waitFor(() => {
    expect(screen.getByText("期末模拟考试")).toBeInTheDocument();
    expect(screen.getByText("第一阶段测试考试")).toBeInTheDocument();
  });

  expect(screen.getByText("共收录 2 场考试任务 · 考试交卷不可逆，请在答题面板谨慎确认")).toBeInTheDocument();
  expect(screen.getByText(/90 分钟/)).toBeInTheDocument();
  expect(screen.getByText("已批改: 95分")).toBeInTheDocument();
});

it("prompts confirmation before starting exam to prevent accidental countdown trigger", async () => {
  render(<ExamListPanel courses={mockCourses} ready={true} />);

  await waitFor(() => {
    expect(screen.getByText("期末模拟考试")).toBeInTheDocument();
  });

  // 1. 点击进入考试
  const enterBtn = screen.getByRole("button", { name: "进入考试" });
  await act(async () => {
    fireEvent.click(enterBtn);
  });

  // 2. 必须先弹出开始前二次确认弹窗
  await waitFor(() => {
    expect(screen.getByText("确认现在开始考试？")).toBeInTheDocument();
    expect(screen.getByText(/该考试设置有限时 90 分钟。一旦确认开始，试卷倒计时将立即触发且不可暂停！/)).toBeInTheDocument();
  });

  // 尚未确认前，不应调用获取试卷
  expect(invoke).not.toHaveBeenCalledWith("get_uooc_exam_paper", expect.anything());

  // 3. 点击确认开始
  const confirmStartBtn = screen.getByRole("button", { name: "确认开始" });
  await act(async () => {
    fireEvent.click(confirmStartBtn);
  });

  // 4. 正式触发开始与试卷加载
  await waitFor(() => {
    expect(invoke).toHaveBeenCalledWith("get_uooc_exam_paper", {
      cid: "101",
      taskId: "t1001",
    });
    expect(screen.getByText("TCP 协议建立连接需要几次握手？")).toBeInTheDocument();
    expect(screen.getByText("题库命中 (100%)")).toBeInTheDocument();
  });
});

it("allows directly viewing submitted exams without countdown warning dialog", async () => {
  render(<ExamListPanel courses={mockCourses} ready={true} />);

  await waitFor(() => {
    expect(screen.getByText("第一阶段测试考试")).toBeInTheDocument();
  });

  // 已交卷的考试操作是“查看答卷”
  const viewBtn = screen.getByRole("button", { name: "查看答卷" });
  await act(async () => {
    fireEvent.click(viewBtn);
  });

  // 直接加载试卷，不弹出倒计时警告确认框
  await waitFor(() => {
    expect(screen.queryByText("确认现在开始考试？")).not.toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("get_uooc_exam_paper", {
      cid: "102",
      taskId: "t1002",
    });
  });
});

it("handles draft saving and submit confirmation dialog", async () => {
  render(<ExamListPanel courses={mockCourses} ready={true} />);

  await waitFor(() => {
    expect(screen.getByText("期末模拟考试")).toBeInTheDocument();
  });

  const enterBtn = screen.getByRole("button", { name: "进入考试" });
  await act(async () => {
    fireEvent.click(enterBtn);
  });

  // 二次确认开始
  const confirmStartBtn = await screen.findByRole("button", { name: "确认开始" });
  await act(async () => {
    fireEvent.click(confirmStartBtn);
  });

  await waitFor(() => {
    expect(screen.getByText("TCP 协议建立连接需要几次握手？")).toBeInTheDocument();
  });

  // 测试暂存草稿
  const saveBtn = screen.getByRole("button", { name: "保存草稿" });
  await act(async () => {
    fireEvent.click(saveBtn);
  });

  await waitFor(() => {
    expect(invoke).toHaveBeenCalledWith("save_uooc_exam_draft", expect.objectContaining({
      cid: "101",
      taskId: "t1001",
    }));
  });

  // 测试交卷弹窗
  const submitBtn = screen.getByRole("button", { name: "提交试卷" });
  await act(async () => {
    fireEvent.click(submitBtn);
  });

  await waitFor(() => {
    expect(screen.getByText("确认提交试卷？")).toBeInTheDocument();
    expect(screen.getByText(/仍有未作答题目！/)).toBeInTheDocument();
  });

  // 确认交卷
  const confirmSubmitBtn = screen.getByRole("button", { name: "确认提交" });
  await act(async () => {
    fireEvent.click(confirmSubmitBtn);
  });

  await waitFor(() => {
    expect(invoke).toHaveBeenCalledWith("submit_uooc_exam", expect.objectContaining({
      cid: "101",
      taskId: "t1001",
    }));
  });
});
