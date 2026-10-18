/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { QuizPanel } from "./QuizPanel";
import type { QuizState, UoocSnapshot } from "./types";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
function quiz(): QuizState {
  return {
    accountId: "7",
    cid: "42",
    taskId: "99",
    resourceId: "21",
    title: "测验",
    mode: "approval",
    stage: "awaitingApproval",
    revision: 7,
    questions: [
      {
        id: "q1",
        text: "2+2?",
        kind: "single",
        options: [
          { id: "a", text: "4" },
          { id: "b", text: "5" },
        ],
        blanks: 0,
        images: [],
      },
    ],
    answers: { q1: ["b"] },
    reviews: [],
    message: "请确认答案",
    gated: true,
    videoComplete: false,
    passed: [],
  };
}
function response(q: QuizState): UoocSnapshot {
  return {
    revision: 20,
    session: "ready",
    user: { id: "7", name: "测试" },
    courses: [],
    tasks: [],
    settings: {
      concurrency: 3,
      reportIntervalSec: 20,
      speedMultiplier: 2,
      dryRun: false,
      discussionEnabled: true,
      discussionContent: "要好好学习",
      discussionIntervalMin: 30,
    },
    message: "",
    logs: [],
    quizzes: { "7/42": q },
  };
}
it("submits the edited answers with the newly saved approval revision", async () => {
  const q = quiz();
  vi.mocked(invoke).mockImplementation(async (name) =>
    name === "save_uooc_quiz_answers"
      ? response({ ...q, revision: 8, answers: { q1: ["a"] } })
      : response({ ...q, revision: 9, stage: "submitting" }),
  );
  render(<QuizPanel quiz={q} active={false} dryRun={false} weight={20} onSnapshot={vi.fn()} />);
  fireEvent.click(screen.getByRole("radio", { name: "4" }));
  fireEvent.click(screen.getByRole("button", { name: "确认答案并提交" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("control_uooc_quiz", {
      cid: "42",
      revision: 8,
      action: "submit",
    }),
  );
  expect(invoke).toHaveBeenCalledWith("save_uooc_quiz_answers", {
    cid: "42",
    revision: 7,
    answers: { q1: ["a"] },
  });
});
it.each([0, null, undefined])("disables answering when quiz weight is %s", (weight) => {
  render(
    <QuizPanel quiz={quiz()} active={false} dryRun={false} weight={weight} onSnapshot={vi.fn()} />,
  );
  expect(screen.getByRole("button", { name: "AI 答题" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "确认答案并提交" })).toBeDisabled();
  expect(screen.getByRole("radio", { name: "4" })).toBeDisabled();
});
it("supervision waits for an explicit choice and dry-run cannot submit", () => {
  render(
    <QuizPanel
      quiz={{ ...quiz(), stage: "awaitingChoice", mode: "supervised" }}
      active={false}
      dryRun
      weight={20}
      onSnapshot={vi.fn()}
    />,
  );
  expect(screen.queryByRole("radio")).not.toBeInTheDocument();
  expect(invoke).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "自主答题" }));
  expect(screen.getByRole("radio", { name: "4" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "确认答案并提交" })).toBeDisabled();
});

it("calls control_uooc_quiz with matchQa action when clicking 匹配题库", async () => {
  const q = quiz();
  vi.mocked(invoke).mockResolvedValue(response(q));
  render(<QuizPanel quiz={q} active={false} dryRun={false} weight={20} onSnapshot={vi.fn()} />);
  const matchBtn = screen.getByRole("button", { name: "匹配题库" });
  expect(matchBtn).toBeEnabled();
  fireEvent.click(matchBtn);
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("control_uooc_quiz", {
      cid: "42",
      revision: 7,
      action: "matchQa",
    }),
  );
});

it("allows clicking 查看测验 to force fetch completed quiz when quiz is passed", async () => {
  const passedQuiz: QuizState = {
    ...quiz(),
    stage: "passed",
    message: "当前测验已由服务端确认完成",
    questions: [],
    answers: {},
  };
  const onSnapshot = vi.fn();
  vi.mocked(invoke).mockResolvedValue(
    response({
      ...passedQuiz,
      questions: [
        {
          id: "q1",
          text: "微积分定理证明",
          kind: "single",
          options: [{ id: "a", text: "4" }],
          blanks: 0,
          images: [],
        },
      ],
      answers: { q1: ["a"] },
    }),
  );

  render(
    <QuizPanel
      quiz={passedQuiz}
      active={false}
      dryRun={false}
      weight={20}
      onSnapshot={onSnapshot}
    />,
  );
  const viewBtn = screen.getByRole("button", { name: "查看测验" });
  expect(viewBtn).toBeEnabled();
  fireEvent.click(viewBtn);

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("fetch_uooc_completed_quiz", {
      cid: "42",
      taskId: "99",
      resourceId: "21",
    }),
  );
  expect(onSnapshot).toHaveBeenCalled();
});

it("displays question type chip, option label badges beside options and no bottom annotation", () => {
  render(<QuizPanel quiz={quiz()} active={false} dryRun={false} weight={20} onSnapshot={vi.fn()} />);

  // 1. Question type chip
  expect(screen.getByText("单选题")).toBeInTheDocument();
  // 2. Question index and ID
  expect(screen.getByText("第 1 题")).toBeInTheDocument();
  expect(screen.getByText("ID: q1")).toBeInTheDocument();
  // 3. Option labels (A., B.)
  expect(screen.getByText("A.")).toBeInTheDocument();
  expect(screen.getByText("B.")).toBeInTheDocument();
  // 4. 标注器不在下方显示
  expect(screen.queryByText(/当前作答 \/ 匹配答案/)).not.toBeInTheDocument();
  expect(screen.queryByText(/标准答案：/)).not.toBeInTheDocument();
});

it("displays 标准答案 and 排除答案 beside options and selection modification does not alter standard answer", () => {
  const matchedQuiz: QuizState = {
    ...quiz(),
    answers: { q1: ["a"] },
    reviews: [
      {
        questionId: "q1",
        answers: ["a"],
        probabilities: {},
        message: "题库命中标准答案",
      },
      {
        questionId: "q1",
        answers: ["b"],
        probabilities: {},
        message: "题库已排除错误选项：b，请勿选择",
      },
    ],
  };

  render(<QuizPanel quiz={matchedQuiz} active={false} dryRun={false} weight={20} onSnapshot={vi.fn()} />);

  // Option A displays 标准答案, Option B displays 排除答案
  expect(screen.getByText("标准答案")).toBeInTheDocument();
  expect(screen.getByText("排除答案")).toBeInTheDocument();

  // Initially Option A has 已选
  expect(screen.getByText("已选")).toBeInTheDocument();

  // User clicks Option B to modify answer
  fireEvent.click(screen.getByRole("radio", { name: /5/ }));

  // 标准答案 MUST STILL be on Option A and NOT altered to Option B!
  expect(screen.getByText("标准答案")).toBeInTheDocument();
  expect(screen.getByText("排除答案")).toBeInTheDocument();
});

it("automatically selects option when bank matches standard answer even if answers was initially empty", () => {
  const matchedQuiz: QuizState = {
    ...quiz(),
    answers: {},
    reviews: [
      {
        questionId: "q1",
        answers: ["a"],
        probabilities: {},
        message: "题库命中标准答案",
      },
    ],
  };

  render(<QuizPanel quiz={matchedQuiz} active={false} dryRun={false} weight={20} onSnapshot={vi.fn()} />);

  // Option A automatically displays 标准答案 AND 已选
  expect(screen.getByText("标准答案")).toBeInTheDocument();
  expect(screen.getByText("已选")).toBeInTheDocument();
  const radioA = screen.getByRole("radio", { name: /4/ });
  expect(radioA).toBeChecked();
});

it("pops up execution flow modal with AI session when clicking AI 答题", async () => {
  const q: QuizState = {
    ...quiz(),
    answers: {},
    aiSessions: [
      {
        batchIndex: 1,
        questionIds: ["q1"],
        questionTitles: ["2+2?"],
        model: "gpt-4o-mini",
        prompt: "{\"question\": \"2+2?\"}",
        response: "{\"answers\": {\"q1\": [\"a\"]}}",
        answers: { q1: ["a"] },
        status: "completed",
      },
    ],
  };
  vi.mocked(invoke).mockResolvedValue(response(q));
  render(<QuizPanel quiz={q} active={false} dryRun={false} weight={20} onSnapshot={vi.fn()} />);

  const aiBtn = screen.getByRole("button", { name: "AI 答题" });
  fireEvent.click(aiBtn);

  // Execution flow modal appears with AI session details
  await waitFor(() => {
    expect(screen.getByText("AI 智能答题流程")).toBeInTheDocument();
    expect(screen.getByText(/AI 会话调用 Session/)).toBeInTheDocument();
    expect(screen.getByText(/模型: gpt-4o-mini/)).toBeInTheDocument();
    expect(screen.getByText("{\"question\": \"2+2?\"}")).toBeInTheDocument();
    expect(screen.getByText("{\"answers\": {\"q1\": [\"a\"]}}")).toBeInTheDocument();
  });
});

it("pops up execution flow modal when clicking 匹配题库", async () => {
  const q = quiz();
  vi.mocked(invoke).mockResolvedValue(response(q));
  render(<QuizPanel quiz={q} active={false} dryRun={false} weight={20} onSnapshot={vi.fn()} />);

  const matchBtn = screen.getByRole("button", { name: "匹配题库" });
  fireEvent.click(matchBtn);

  // Execution flow modal appears
  await waitFor(() => {
    expect(screen.getByText("题库匹配流程")).toBeInTheDocument();
    expect(screen.getByText(/题库平台比对/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "查看试卷与作答结果" })).toBeInTheDocument();
  });
});
