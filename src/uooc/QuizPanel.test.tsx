/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { QuizPanel } from "./QuizPanel";
import type { QuizState, UoocSnapshot } from "./types";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
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
