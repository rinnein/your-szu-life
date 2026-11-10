/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  renderHook,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { copyText } from "../clipboard";
import { SampleField, resetWhitespaceVisible } from "./SampleField";
import { JudgeResult, Verdict } from "./JudgeResult";
import { isPending, judgeDetails, problemVerdict } from "./judging";
import { useSubmissions } from "./useSubmissions";
import { problemMarkdown } from "./content";
import type { Paper, Problem, Records } from "./types";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock("../clipboard", () => ({ copyText: vi.fn().mockResolvedValue(true) }));
const p: Problem = {
  id: "1",
  displayLabel: "A",
  kind: 0,
  title: "求和",
  description: "计算",
  input: "",
  output: "",
  hint: "",
  timeLimit: 1,
  memoryLimit: 128,
  samples: [],
  choices: [],
  multiple: false,
  blanks: 0,
  languages: ["cpp"],
  templates: {},
  ioMode: "stdio",
};
const second = { ...p, id: "2", displayLabel: "B" };
const paper = {
  account: "student",
  epoch: 1,
  problems: [p, second],
  contest: { title: "测试" },
  sourceUrl: "http://oj.test/",
} as Paper;
const pending = { id: 11, problem: 1, status: 6 };
const finished = {
  id: 11,
  problem: 1,
  status: 0,
  result: {
    data: [
      { test_case: "1", result: 0, cpu_time: 0, real_time: 1, memory: 1048576 },
      { test_case: "2", result: 1, cpu_time: 1001, real_time: 1002, memory: 2097152 },
    ],
  },
};
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
  vi.stubGlobal(
    "matchMedia",
    vi.fn().mockReturnValue({
      matches: false,
      addEventListener() {},
      removeEventListener() {},
      addListener() {},
      removeListener() {},
    }),
  );
  vi.mocked(invoke).mockResolvedValue({ records: [], uncertain: [] });
});
afterEach(() => {
  cleanup();
  resetWhitespaceVisible();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

it.each([
  [0, "AC", "success"],
  [-1, "WA", "danger"],
  [1, "TLE", "warning"],
  [2, "TLE", "warning"],
  [-2, "CE", "warning"],
  [3, "MLE", "warning"],
  [4, "RE", "danger"],
])("shows ACM verdict %s", (status, code, color) => {
  render(<Verdict status={Number(status)} />);
  expect(screen.getByText(String(code)).closest(".md-chip")).toHaveAttribute("data-color", color);
});
it("keeps solved status and otherwise uses latest attempt, including zero-valued AC", () => {
  expect(
    problemVerdict([
      { id: 1, status: 0 },
      { id: 2, status: -1 },
    ]),
  ).toBe(0);
  expect(
    problemVerdict([
      { id: 1, status: -1 },
      { id: 2, status: 1 },
    ]),
  ).toBe(1);
  expect(problemVerdict([])).toBeUndefined();
  expect(problemVerdict([{ id: 2 }])).toBe(6);
  expect(isPending({ id: 1, status: 0 })).toBe(false);
});
it("copies original sample whitespace and leaves read-only fields selectable", async () => {
  const value = "\n  1 \t\r\n2  \r\n\r\n";
  render(<SampleField sample={1} label="输入" value={value} />);
  const field = screen.getByRole("textbox", { name: "样例 1 输入" });
  expect(field).toHaveAttribute("readonly");
  expect(field).toHaveValue(value.replace(/\r\n/g, "\n"));
  fireEvent.click(screen.getByRole("button", { name: "复制样例 1 输入" }));
  await waitFor(() => expect(copyText).toHaveBeenCalledWith(value));
  const markdown = problemMarkdown({ ...p, samples: [{ input: value, output: "  \t" }] }, paper);
  expect(markdown).toContain(value);
  expect(markdown).toContain("  \t\n```");
});
it("toggles visible spaces in sample field via overlay and handles original symbols", async () => {
  const value = "1  ·  2 \t→ 3 ";
  render(<SampleField sample={1} label="输出" value={value} />);
  const field = screen.getByRole("textbox", { name: "样例 1 输出" }) as HTMLTextAreaElement;
  expect(field).toHaveValue(value);

  // Before toggle, overlay is not mounted
  expect(screen.queryByTestId("whitespace-overlay")).toBeNull();

  const toggleBtn = screen.getByRole("button", { name: "显示样例 1 输出空格" });
  fireEvent.click(toggleBtn);

  // After toggle, overlay is mounted
  const overlay = screen.getByTestId("whitespace-overlay");
  expect(overlay).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "隐藏样例 1 输出空格" })).toBeInTheDocument();

  // The underlying textarea still has the untouched original value (including original '·' and '→')
  expect(field).toHaveValue(value);

  // Overlay contains space indicators and tab indicators
  const spaceTokens = overlay.querySelectorAll('[data-whitespace="space"]');
  expect(spaceTokens.length).toBeGreaterThan(0);
  expect(spaceTokens[0]).toHaveTextContent("··");

  const tabTokens = overlay.querySelectorAll('[data-whitespace="tab"]');
  expect(tabTokens.length).toBe(1);

  // The original symbols '·' and '→' are preserved as transparent text tokens, not whitespace indicators
  const transparentTokens = overlay.querySelectorAll(".text-transparent");
  const tokenTexts = Array.from(transparentTokens).map((el) => el.textContent);
  expect(tokenTexts).toContain("·");
  expect(tokenTexts).toContain("→");

  // Copying via button still copies the original untouched string
  fireEvent.click(screen.getByRole("button", { name: "复制样例 1 输出" }));
  await waitFor(() => expect(copyText).toHaveBeenCalledWith(value));

  // Toggle off unmounts overlay
  fireEvent.click(screen.getByRole("button", { name: "隐藏样例 1 输出空格" }));
  expect(screen.queryByTestId("whitespace-overlay")).toBeNull();
  expect(field).toHaveValue(value);
});

it("co-scrolls textarea and overlay in one scroll container without sync", () => {
  render(<SampleField sample={1} label="输入" value={"1\n2\n3"} />);
  fireEvent.click(screen.getByRole("button", { name: "显示样例 1 输入空格" }));
  const field = screen.getByRole("textbox", { name: "样例 1 输入" });
  const overlay = screen.getByTestId("whitespace-overlay");
  const scroller = overlay.closest(".md-sample-field");
  // 文本层与标记层是同一滚动容器的孩子：滚动天然同步，无需事件复制
  expect(scroller).not.toBeNull();
  expect(scroller).toContainElement(field);
  // 文本层自身不滚动：行数全部展开（rows = 总行数），溢出裁剪交给外层容器
  expect(field).toHaveAttribute("rows", "3");
  expect(field.style.overflow).toBe("hidden");
});

it("renders whitespace markers with column-aware exact advances", () => {
  // 行内 tab 在第 1 列 → 原生制表位前进 7 列；连续空格 2 个 → 2 列
  render(<SampleField sample={1} label="输出" value={"a\tb  c"} />);
  fireEvent.click(screen.getByRole("button", { name: "显示样例 1 输出空格" }));
  const overlay = screen.getByTestId("whitespace-overlay");
  const tab = overlay.querySelector('[data-whitespace="tab"]');
  expect(tab).not.toBeNull();
  expect(tab!.getAttribute("style")).toContain("width: 7ch");
  const space = overlay.querySelector('[data-whitespace="space"]');
  expect(space).not.toBeNull();
  expect(space!.getAttribute("style")).toContain("width: 2ch");
});
it("renders structured cases, units and safely escaped compiler errors", () => {
  const view = render(<JudgeResult submission={finished} polling={false} onRefresh={() => {}} />);
  expect(screen.getByRole("grid", { name: "测试点判题结果" })).toHaveTextContent("1001 ms");
  expect(screen.getByRole("grid")).toHaveTextContent("2.00 MB");
  expect(screen.getByRole("progressbar", { name: /测试点通过比例/ })).toHaveAttribute(
    "aria-valuenow",
    "1",
  );
  expect(judgeDetails(finished).cpu).toBe(1001);
  view.rerender(
    <JudgeResult
      submission={{ id: 1, status: -2, result: '<img src=x onerror="bad()">' }}
      polling={false}
      onRefresh={() => {}}
    />,
  );
  expect(screen.getByText('<img src=x onerror="bad()">')).toBeInTheDocument();
  expect(document.querySelector("img")).toBeNull();
  expect(screen.queryByRole("grid")).toBeNull();
});
it("isolates current-problem history and ignores late responses after switching", async () => {
  let resolveFirst: (value: Records) => void = () => {};
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    const a = args as { problemId?: string };
    if (command === "get_oj_records" && a.problemId === "1")
      return new Promise<Records>((resolve) => {
        resolveFirst = resolve;
      });
    return {
      records: [
        { id: 21, problem: 2, status: -1 },
        { id: 12, problem: 1, status: 0 },
      ],
      uncertain: [],
    };
  });
  const hook = renderHook(({ problem }) => useSubmissions("site", "12", paper, problem), {
    initialProps: { problem: p },
  });
  hook.rerender({ problem: second });
  await waitFor(() => expect(hook.result.current.records?.records.map((r) => r.id)).toEqual([21]));
  await act(async () =>
    resolveFirst({ records: [{ id: 12, problem: 1, status: 0 }], uncertain: [] }),
  );
  expect(hook.result.current.records?.records.map((r) => r.id)).toEqual([21]);
  expect(invoke).toHaveBeenCalledWith("get_oj_records", {
    siteId: "site",
    contestId: "12",
    kind: 0,
    problemId: "2",
  });
});
it("resumes a pending result and stops when the backend channel reports a final verdict", async () => {
  let channel: { onmessage: ((message: unknown) => void) | null } | undefined;
  let watchCalls = 0;
  let resolveWatch!: () => void;
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "watch_oj_result") {
      watchCalls++;
      channel = (args as { onResult?: typeof channel })?.onResult;
      return new Promise<void>((resolve) => {
        resolveWatch = resolve;
      });
    }
    return { records: [pending], uncertain: [] };
  });
  const hook = renderHook(() => useSubmissions("site", "12", paper, p));
  await act(async () => {});
  await waitFor(() => expect(watchCalls).toBe(1));
  expect(hook.result.current.polling).toBe(true);

  await act(async () => channel?.onmessage?.({ ...pending, status: 7 }));
  expect(hook.result.current.polling).toBe(true);

  await act(async () => channel?.onmessage?.(finished));
  expect(hook.result.current.result?.status).toBe(0);
  expect(hook.result.current.verdict(p)).toBe(0);
  expect(hook.result.current.polling).toBe(false);
  await act(async () => resolveWatch());
  expect(invoke).not.toHaveBeenCalledWith("submit_oj", expect.anything());
});
it("cancels the backend watch when the problem changes", async () => {
  let channel: { onmessage: ((message: unknown) => void) | null } | undefined;
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "watch_oj_result") {
      channel = (args as { onResult?: typeof channel })?.onResult;
      return new Promise<void>(() => {});
    }
    return { records: [], uncertain: [] };
  });
  const hook = renderHook(({ problem }) => useSubmissions("site", "12", paper, problem), {
    initialProps: { problem: p },
  });
  await act(async () => {});
  await act(async () => hook.result.current.show(pending));
  await waitFor(() => expect(channel).toBeDefined());
  hook.rerender({ problem: second });
  await act(async () => channel?.onmessage?.(finished));
  expect(hook.result.current.result).toBeUndefined();
  hook.unmount();
});
it("surfaces backend read failures and allows an explicit retry", async () => {
  let watchCalls = 0;
  let rejectFirst!: (error: unknown) => void;
  let secondChannel: { onmessage: ((message: unknown) => void) | null } | undefined;
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "watch_oj_result") {
      watchCalls++;
      if (watchCalls === 1) {
        return new Promise<void>((_, reject) => {
          rejectFirst = reject;
        });
      }
      secondChannel = (args as { onResult?: typeof secondChannel })?.onResult;
      return undefined;
    }
    return { records: [], uncertain: [] };
  });
  const hook = renderHook(() => useSubmissions("site", "12", paper, p));
  await act(async () => {});
  await act(async () => hook.result.current.show(pending));
  await waitFor(() => expect(watchCalls).toBe(1));
  await act(async () => rejectFirst("获取判题结果失败：network；不会重新提交代码。"));
  expect(hook.result.current.polling).toBe(false);
  expect(hook.result.current.pollError).toContain("不会重新提交代码");

  await act(async () => hook.result.current.show(pending));
  await waitFor(() => expect(secondChannel).toBeDefined());
  await act(async () => secondChannel?.onmessage?.(finished));
  expect(hook.result.current.result?.status).toBe(0);
});
it("receives CPH results only for the current account generation", async () => {
  let channel: { onmessage: ((message: unknown) => void) | null } | undefined;
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "watch_oj_result") {
      channel = (args as { onResult?: typeof channel })?.onResult;
      return new Promise<void>(() => {});
    }
    return { records: [], uncertain: [] };
  });
  const hook = renderHook(() => useSubmissions("site", "12", paper, p));
  await act(async () => {});
  const callback = vi
    .mocked(listen)
    .mock.calls.find(([event]) => event === "oj-companion-result")![1];
  const payload = {
    siteId: "site",
    contestId: "12",
    problemKey: "0:1",
    account: "student",
    epoch: 1,
    submission: pending,
  };
  await act(async () =>
    callback({ event: "oj-companion-result", id: 1, payload: { ...payload, epoch: 0 } }),
  );
  expect(hook.result.current.result).toBeUndefined();
  await act(async () => callback({ event: "oj-companion-result", id: 1, payload }));
  await waitFor(() => expect(channel).toBeDefined());
  await act(async () => channel?.onmessage?.(finished));
  expect(hook.result.current.result?.status).toBe(0);
});

it("extracts stdout/output and stdin/input per test case across field variants", () => {
  const submission = {
    id: 1,
    status: 0,
    result: {
      data: [
        { test_case: "1", result: 0, output: "line1\nline2", input: "1 2\n" },
        { test_case: "2", result: 0, stdout: "test2 stdout", stdin: "3 4\n" },
        { test_case: "3", result: 0, user_output: "test3 output", test_input: "5 6\n" },
        { test_case: "4", result: 0, user_out: "test4 output", user_input: "7 8\n" },
        { test_case: "5", result: 0 },
      ],
    },
  };
  const details = judgeDetails(submission);
  expect(details.cases[0].output).toBe("line1\nline2");
  expect(details.cases[0].input).toBe("1 2\n");
  expect(details.cases[1].output).toBe("test2 stdout");
  expect(details.cases[1].input).toBe("3 4\n");
  expect(details.cases[2].output).toBe("test3 output");
  expect(details.cases[2].input).toBe("5 6\n");
  expect(details.cases[3].output).toBe("test4 output");
  expect(details.cases[3].input).toBe("7 8\n");
  expect(details.cases[4].output).toBeUndefined();
  expect(details.cases[4].input).toBeUndefined();
});

it("expands and collapses checkpoint input and output and copies them independently", async () => {
  const submissionWithIO = {
    id: 99,
    status: 0,
    result: {
      data: [
        {
          test_case: "1",
          result: 0,
          cpu_time: 10,
          real_time: 12,
          memory: 1048576,
          input: "10 20\n",
          output: "Server stdout 1",
        },
        { test_case: "2", result: 0, cpu_time: 10, real_time: 12, memory: 1048576 },
      ],
    },
  };
  render(<JudgeResult submission={submissionWithIO} polling={false} onRefresh={() => {}} />);

  // Initially, input and output are not visible
  expect(screen.queryByRole("textbox", { name: "测试点 1 输入" })).toBeNull();
  expect(screen.queryByRole("textbox", { name: "测试点 1 输出" })).toBeNull();

  // Find the toggle button or row for test case 1 and click to expand
  const toggleBtn1 = screen.getByRole("button", { name: "展开测试点 1 详情" });
  fireEvent.click(toggleBtn1);

  // Now both input and output textareas are displayed
  const inputField = screen.getByRole("textbox", { name: "测试点 1 输入" }) as HTMLTextAreaElement;
  const outputField = screen.getByRole("textbox", { name: "测试点 1 输出" }) as HTMLTextAreaElement;
  expect(inputField).toHaveValue("10 20\n");
  expect(outputField).toHaveValue("Server stdout 1");
  expect(screen.getByText("测试点 1 输入 (stdin)")).toBeInTheDocument();
  expect(screen.getByText("测试点 1 在线输出 (stdout)")).toBeInTheDocument();

  // Click copy input button
  const copyInputBtn = screen.getByRole("button", { name: "复制测试点 1 输入" });
  fireEvent.click(copyInputBtn);
  await waitFor(() => expect(copyText).toHaveBeenCalledWith("10 20\n"));

  // Click copy output button
  const copyOutputBtn = screen.getByRole("button", { name: "复制测试点 1 输出" });
  fireEvent.click(copyOutputBtn);
  await waitFor(() => expect(copyText).toHaveBeenCalledWith("Server stdout 1"));

  // Click to collapse
  const collapseBtn1 = screen.getByRole("button", { name: "收起测试点 1 详情" });
  fireEvent.click(collapseBtn1);
  expect(screen.queryByRole("textbox", { name: "测试点 1 输入" })).toBeNull();
  expect(screen.queryByRole("textbox", { name: "测试点 1 输出" })).toBeNull();
});

it("shares whitespace visibility state globally across sample fields and checkpoints", () => {
  render(
    <div>
      <SampleField sample={1} label="输入" value="sample 1  spaces" />
      <SampleField label="测试点 1 输出" value="check 1  spaces" ariaLabel="测试点 1 输出" />
    </div>,
  );

  // Both overlays initially not mounted
  expect(screen.queryAllByTestId("whitespace-overlay")).toHaveLength(0);

  // Toggle on first field
  fireEvent.click(screen.getByRole("button", { name: "显示样例 1 输入空格" }));

  // Both fields now have overlays mounted due to shared global state
  expect(screen.getAllByTestId("whitespace-overlay")).toHaveLength(2);
  expect(screen.getByRole("button", { name: "隐藏样例 1 输入空格" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "隐藏测试点 1 输出空格" })).toBeInTheDocument();

  // Toggle off from the second field
  fireEvent.click(screen.getByRole("button", { name: "隐藏测试点 1 输出空格" }));

  // Both fields now unmount their overlays
  expect(screen.queryAllByTestId("whitespace-overlay")).toHaveLength(0);
  expect(screen.getByRole("button", { name: "显示样例 1 输入空格" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "显示测试点 1 输出空格" })).toBeInTheDocument();
});

it("shows placeholder hints and disables copy when checkpoint has neither input nor stdout", () => {
  const submissionEmpty = {
    id: 99,
    status: 0,
    result: {
      data: [{ test_case: "1", result: 0, cpu_time: 10, real_time: 12, memory: 1048576 }],
    },
  };
  render(<JudgeResult submission={submissionEmpty} polling={false} onRefresh={() => {}} />);

  const toggleBtn1 = screen.getByRole("button", { name: "展开测试点 1 详情" });
  fireEvent.click(toggleBtn1);

  expect(screen.getByText("（服务端未返回测试输入）")).toBeInTheDocument();
  expect(screen.getByText("（服务端未返回标准输出）")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "复制测试点 1 输入" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "复制测试点 1 输出" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "显示测试点 1 输入空格" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "显示测试点 1 输出空格" })).toBeDisabled();
});
