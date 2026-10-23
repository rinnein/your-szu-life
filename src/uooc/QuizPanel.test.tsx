/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { QuizPanel, getQuizCacheKey, loadQuizCache, saveQuizCache, stripHtml } from "./QuizPanel";
import type { QuizState, UoocSnapshot } from "./types";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
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
  localStorage.clear();
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
  fireEvent.click(screen.getByRole("button", { name: "提交答案" }));
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
  expect(screen.getByRole("button", { name: "提交答案" })).toBeDisabled();
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
  expect(screen.getByRole("button", { name: "提交答案" })).toBeDisabled();
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
  render(
    <QuizPanel quiz={quiz()} active={false} dryRun={false} weight={20} onSnapshot={vi.fn()} />,
  );

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

  render(
    <QuizPanel quiz={matchedQuiz} active={false} dryRun={false} weight={20} onSnapshot={vi.fn()} />,
  );

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

  render(
    <QuizPanel quiz={matchedQuiz} active={false} dryRun={false} weight={20} onSnapshot={vi.fn()} />,
  );

  // Option A automatically displays 标准答案 AND 已选
  expect(screen.getByText("标准答案")).toBeInTheDocument();
  expect(screen.getByText("已选")).toBeInTheDocument();
  const radioA = screen.getByRole("radio", { name: /4/ });
  expect(radioA).toBeChecked();
});

it("correctly displays student answers and exclusions without falsifying wrong options as standard answers in passed quiz", () => {
  const passedQuizWithWrongs: QuizState = {
    ...quiz(),
    stage: "passed",
    message: "测验已完成，得分：70.00 / 100.00 分",
    // q1 was answered correctly with option "a"
    // q2 was answered incorrectly with option "b", so answers does not contain q2!
    answers: { q1: ["a"] },
    studentAnswers: { q1: ["a"], q2: ["b"] },
    questions: [
      {
        id: "q1",
        text: "第一道题目（做对）",
        kind: "single",
        options: [
          { id: "a", text: "正确选项内容" },
          { id: "b", text: "干扰项" },
        ],
        blanks: 0,
        images: [],
      },
      {
        id: "q2",
        text: "第二道题目（做错）",
        kind: "single",
        options: [
          { id: "a", text: "真·标准答案内容" },
          { id: "b", text: "用户选错的选项内容" },
        ],
        blanks: 0,
        images: [],
      },
    ],
    reviews: [
      {
        questionId: "q2",
        answers: ["b"],
        probabilities: {},
        message: "作答未得分，已排除错误选项：b",
      },
    ],
  };

  render(
    <QuizPanel
      quiz={passedQuizWithWrongs}
      active={false}
      dryRun={false}
      weight={20}
      onSnapshot={vi.fn()}
    />,
  );

  // 1. Overall score in message
  expect(screen.getByText("测验已完成，得分：70.00 / 100.00 分")).toBeInTheDocument();

  // 2. Q1 (correct): Option a has 标准答案 and 已选
  expect(screen.getByText("标准答案")).toBeInTheDocument();

  // 3. Q2 (wrong): Option b has 排除答案 and 已选, and DOES NOT have 标准答案!
  expect(screen.getByText("排除答案")).toBeInTheDocument();
  expect(screen.getByText("作答未得分，已排除错误选项：b")).toBeInTheDocument();
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
        prompt: '{"question": "2+2?"}',
        response: '{"answers": {"q1": ["a"]}}',
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
    expect(screen.getByText('{"question": "2+2?"}')).toBeInTheDocument();
    expect(screen.getByText('{"answers": {"q1": ["a"]}}')).toBeInTheDocument();
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

it("automatically caches selected radio option to localStorage upon modification", () => {
  const q = quiz();
  const cacheKey = getQuizCacheKey(q.accountId, q.cid, q.taskId, q.resourceId);
  expect(loadQuizCache(cacheKey)).toBeNull();

  render(<QuizPanel quiz={q} active={false} dryRun={false} weight={20} onSnapshot={vi.fn()} />);
  fireEvent.click(screen.getByRole("radio", { name: "4" }));

  expect(loadQuizCache(cacheKey)).toEqual({ q1: ["a"] });
});

it("automatically caches multiple choice checkboxes to localStorage upon modification", () => {
  const multiQuiz: QuizState = {
    ...quiz(),
    questions: [
      {
        id: "q_multi",
        text: "下列哪些属于偶数？",
        kind: "multiple",
        options: [
          { id: "a", text: "2" },
          { id: "b", text: "3" },
          { id: "c", text: "4" },
        ],
        blanks: 0,
        images: [],
      },
    ],
    answers: { q_multi: ["a"] },
  };
  const cacheKey = getQuizCacheKey(
    multiQuiz.accountId,
    multiQuiz.cid,
    multiQuiz.taskId,
    multiQuiz.resourceId,
  );

  render(
    <QuizPanel quiz={multiQuiz} active={false} dryRun={false} weight={20} onSnapshot={vi.fn()} />,
  );
  fireEvent.click(screen.getByRole("checkbox", { name: "4" }));

  const cached = loadQuizCache(cacheKey);
  expect(cached?.q_multi).toContain("a");
  expect(cached?.q_multi).toContain("c");
});

it("automatically caches fill-in-the-blank text to localStorage upon modification", () => {
  const textQuiz: QuizState = {
    ...quiz(),
    questions: [
      {
        id: "q_text",
        text: "中国的首都是______。",
        kind: "text",
        options: [],
        blanks: 1,
        images: [],
      },
    ],
    answers: {},
  };
  const cacheKey = getQuizCacheKey(
    textQuiz.accountId,
    textQuiz.cid,
    textQuiz.taskId,
    textQuiz.resourceId,
  );

  render(
    <QuizPanel quiz={textQuiz} active={false} dryRun={false} weight={20} onSnapshot={vi.fn()} />,
  );
  const textarea = screen.getByRole("textbox");
  fireEvent.change(textarea, { target: { value: "北京" } });

  expect(loadQuizCache(cacheKey)).toEqual({ q_text: ["北京"] });
});

it("restores cached answers from localStorage on mount", () => {
  const q = quiz();
  const cacheKey = getQuizCacheKey(q.accountId, q.cid, q.taskId, q.resourceId);
  saveQuizCache(cacheKey, { q1: ["a"] });

  render(<QuizPanel quiz={q} active={false} dryRun={false} weight={20} onSnapshot={vi.fn()} />);
  const radioA = screen.getByRole("radio", { name: /4/ });
  expect(radioA).toBeChecked();
});

it("manually clicking 上传草稿 saves answers locally and uploads draft to uooc server", async () => {
  const q = quiz();
  vi.mocked(invoke).mockImplementation(async (name) => {
    if (name === "save_uooc_quiz_answers") {
      return response({ ...q, revision: 8, answers: { q1: ["a"] } });
    }
    if (name === "save_uooc_exam_draft") {
      return undefined;
    }
    return response(q);
  });

  render(<QuizPanel quiz={q} active={false} dryRun={false} weight={20} onSnapshot={vi.fn()} />);
  fireEvent.click(screen.getByRole("radio", { name: "4" }));
  fireEvent.click(screen.getByRole("button", { name: "上传草稿" }));

  await waitFor(() => {
    expect(invoke).toHaveBeenCalledWith("save_uooc_quiz_answers", {
      cid: "42",
      revision: 7,
      answers: { q1: ["a"] },
    });
    expect(invoke).toHaveBeenCalledWith("save_uooc_exam_draft", {
      cid: "42",
      taskId: "99",
      answers: { q1: ["a"] },
    });
  });
});

it("automatically saves answers to server when modal closes (unmount) if answers were modified", async () => {
  const q = quiz();
  vi.mocked(invoke).mockResolvedValue(response({ ...q, revision: 8, answers: { q1: ["a"] } }));

  const { unmount } = render(
    <QuizPanel quiz={q} active={false} dryRun={false} weight={20} onSnapshot={vi.fn()} />,
  );

  // Modify option
  fireEvent.click(screen.getByRole("radio", { name: "4" }));
  expect(invoke).not.toHaveBeenCalledWith("save_uooc_quiz_answers", expect.anything());

  // Modal closes (unmount)
  unmount();

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("save_uooc_quiz_answers", {
      cid: "42",
      revision: 7,
      answers: { q1: ["a"] },
    }),
  );
});

it("does not save to server on unmount if answers were not modified", () => {
  const q = quiz();
  const { unmount } = render(
    <QuizPanel quiz={q} active={false} dryRun={false} weight={20} onSnapshot={vi.fn()} />,
  );

  unmount();

  expect(invoke).not.toHaveBeenCalledWith("save_uooc_quiz_answers", expect.anything());
});

it("does not render a bottom 关闭 button in footer", () => {
  const q = quiz();
  render(
    <QuizPanel
      quiz={q}
      active={false}
      dryRun={false}
      weight={20}
      onSnapshot={vi.fn()}
      onClose={vi.fn()}
    />,
  );

  expect(screen.queryByRole("button", { name: "关闭" })).not.toBeInTheDocument();
});

it("opens portal on 在官网打开试卷 and automatically switches to 同步官网试卷数据 as preference", async () => {
  const q = quiz();
  vi.mocked(invoke).mockResolvedValue(response(q));
  render(<QuizPanel quiz={q} active={false} dryRun={false} weight={20} onSnapshot={vi.fn()} />);

  const portalBtn = screen.getByRole("button", { name: "在官网打开试卷" });
  expect(portalBtn).toBeInTheDocument();
  fireEvent.click(portalBtn);

  await waitFor(() => {
    expect(invoke).toHaveBeenCalledWith("open_uooc_quiz_portal", { cid: "42" });
  });

  // Automatically switched to 同步官网试卷数据 as primary action
  await waitFor(() => {
    expect(screen.getByRole("button", { name: "同步官网试卷数据" })).toBeInTheDocument();
  });

  // Clicking 同步官网试卷数据 runs control("check")
  fireEvent.click(screen.getByRole("button", { name: "同步官网试卷数据" }));
  await waitFor(() => {
    expect(invoke).toHaveBeenCalledWith("control_uooc_quiz", {
      cid: "42",
      revision: 7,
      action: "check",
    });
  });
});

it("allows clicking 在官网打开试卷 even when quiz stage is passed and weight is 0", async () => {
  const passedQuiz: QuizState = {
    ...quiz(),
    stage: "passed",
    message: "当前测验已由服务端确认完成",
  };
  vi.mocked(invoke).mockResolvedValue(response(passedQuiz));
  render(
    <QuizPanel quiz={passedQuiz} active={false} dryRun={false} weight={0} onSnapshot={vi.fn()} />,
  );

  const portalBtns = screen.getAllByRole("button", { name: "在官网打开试卷" });
  expect(portalBtns.length).toBeGreaterThan(0);
  expect(portalBtns[0]).toBeEnabled();

  fireEvent.click(portalBtns[0]);

  await waitFor(() => {
    expect(invoke).toHaveBeenCalledWith("open_uooc_quiz_portal", { cid: "42" });
  });
});

it("displays AI reasoning and explanation under question when review contains 理由/解析", () => {
  const quizWithReasoning: QuizState = {
    ...quiz(),
    reviews: [
      {
        questionId: "q1",
        answers: ["a"],
        probabilities: {},
        message:
          "AI 模型 (deepseek-r1) 判定推荐答案\n理由/解析：根据微积分基本定理，连续函数必可积，故选A。",
      },
    ],
  };

  render(
    <QuizPanel
      quiz={quizWithReasoning}
      active={false}
      dryRun={false}
      weight={20}
      onSnapshot={vi.fn()}
    />,
  );
  expect(screen.getByText(/AI 模型 \(deepseek-r1\) 判定推荐答案/)).toBeInTheDocument();
  expect(screen.getByText(/理由\/解析：根据微积分基本定理/)).toBeInTheDocument();
});

it("accurately marks option badges and selects option when options contain HTML tags and bank/AI returns plain text", () => {
  const htmlTaggedQuiz: QuizState = {
    ...quiz(),
    answers: {},
    questions: [
      {
        id: "q_html",
        text: "<p>下列关于微积分说法正确的是？</p>",
        kind: "single",
        options: [
          { id: "A", text: "<p>连续函数一定可积</p>" },
          { id: "B", text: "<span>可积函数一定连续</span>" },
        ],
        blanks: 0,
        images: [],
      },
    ],
    reviews: [
      {
        questionId: "q_html",
        // Bank / AI returns plain text without HTML tags!
        answers: ["连续函数一定可积"],
        probabilities: {},
        message: "题库命中标准答案",
      },
    ],
  };

  render(
    <QuizPanel
      quiz={htmlTaggedQuiz}
      active={false}
      dryRun={false}
      weight={20}
      onSnapshot={vi.fn()}
    />,
  );

  // Option A with HTML tag <p> matches plain text "连续函数一定可积"
  expect(screen.getByText("标准答案")).toBeInTheDocument();
  expect(screen.getByText("已选")).toBeInTheDocument();
  const radioA = screen.getByRole("radio", { name: /连续函数一定可积/ });
  expect(radioA).toBeChecked();
});

it("automatically switches selected option to AI-judged answer upon AI judgment completion", async () => {
  // Initially user had selected option "b" (or initial default)
  const initialQuiz: QuizState = {
    ...quiz(),
    answers: { q1: ["b"] },
    reviews: [],
  };

  // After AI judgment, AI review determines option "a" is the recommendation
  const aiCompletedQuiz: QuizState = {
    ...initialQuiz,
    revision: 8,
    answers: { q1: ["a"] },
    reviews: [
      {
        questionId: "q1",
        answers: ["a"],
        probabilities: {},
        message: "AI 模型 (deepseek-r1) 判定推荐答案",
      },
    ],
  };

  vi.mocked(invoke).mockResolvedValue(response(aiCompletedQuiz));

  const onSnapshot = vi.fn();
  const { rerender } = render(
    <QuizPanel
      quiz={initialQuiz}
      active={false}
      dryRun={false}
      weight={20}
      onSnapshot={onSnapshot}
    />,
  );

  // Initially Option B is selected
  const radioB = screen.getByRole("radio", { name: /5/ });
  expect(radioB).toBeChecked();

  // User clicks AI 答题
  const aiBtn = screen.getByRole("button", { name: "AI 答题" });
  fireEvent.click(aiBtn);

  await waitFor(() => {
    expect(invoke).toHaveBeenCalledWith("control_uooc_quiz", {
      cid: "42",
      revision: 7,
      action: "generate",
    });
  });

  // Re-render with the snapshot returned by AI
  rerender(
    <QuizPanel
      quiz={aiCompletedQuiz}
      active={false}
      dryRun={false}
      weight={20}
      onSnapshot={onSnapshot}
    />,
  );

  // Close the execution modal to return to the paper view
  const viewResultBtn = await screen.findByRole("button", { name: "查看试卷与作答结果" });
  fireEvent.click(viewResultBtn);

  // Option A should now be automatically selected and marked with AI判定!
  await waitFor(() => {
    const radioA = screen.getByRole("radio", { name: /4/ });
    expect(radioA).toBeChecked();
    expect(screen.getByText("AI判定")).toBeInTheDocument();
  });
});

it("stripHtml accurately cleans nested tags, inline style attributes, and preserves math symbols", () => {
  // 1. Nested tags with style attributes
  expect(
    stripHtml(
      "<p style='margin:0;'><span style='color:red;'><strong>选项 A 内容</strong></span><br/></p>",
    ),
  ).toBe("选项 A 内容");

  // 2. Math inequality symbols are strictly preserved without being stripped
  expect(stripHtml("若 x < y 且 z > 0，则下列结论正确的是？")).toBe(
    "若 x < y 且 z > 0，则下列结论正确的是？",
  );

  // 3. Various HTML entities
  expect(stripHtml("第一空&nbsp;&amp;&nbsp;第二空 &lt; 第三空")).toBe("第一空 & 第二空 < 第三空");
});

it("supports toggling HTML tag visibility via the switch in modal header and matches options with nested tags", () => {
  const quizWithNestedTags: QuizState = {
    ...quiz(),
    answers: {},
    questions: [
      {
        id: "q_nested",
        text: "<p>根据定义，<span style='color:blue'><strong>连续函数</strong></span>具有什么性质？</p>",
        kind: "single",
        options: [
          {
            id: "A",
            text: "<p style='font-size:12px;'><span style='color:green;'><strong>一定可积</strong></span></p>",
          },
          { id: "B", text: "<span>不可积</span>" },
        ],
        blanks: 0,
        images: [],
      },
    ],
    reviews: [
      {
        questionId: "q_nested",
        answers: ["一定可积"],
        probabilities: {},
        message: "题库命中标准答案",
      },
    ],
  };

  const { container } = render(
    <QuizPanel
      quiz={quizWithNestedTags}
      active={false}
      dryRun={false}
      weight={20}
      onSnapshot={vi.fn()}
    />,
  );

  // 1. Nested tagged Option A correctly matches standard answer "一定可积"
  expect(screen.getByText("标准答案")).toBeInTheDocument();
  expect(screen.getByText("已选")).toBeInTheDocument();

  // 2. Switch for HTML tags exists in header and is checked by default
  const htmlTagSwitch = screen.getByRole("switch", { name: "显示 HTML 标签" });
  expect(htmlTagSwitch).toBeInTheDocument();
  expect(htmlTagSwitch).toBeChecked();

  // Dimmed tags are visible
  expect(container.querySelectorAll("[data-dimmed-tag='true']").length).toBeGreaterThan(0);

  // 3. Toggle switch to off: dimmed tags become hidden
  fireEvent.click(htmlTagSwitch);
  expect(htmlTagSwitch).not.toBeChecked();
  expect(container.querySelectorAll("[data-dimmed-tag='true']")).toHaveLength(0);

  // 4. Toggle switch back to on
  fireEvent.click(htmlTagSwitch);
  expect(htmlTagSwitch).toBeChecked();
  expect(container.querySelectorAll("[data-dimmed-tag='true']").length).toBeGreaterThan(0);
});
