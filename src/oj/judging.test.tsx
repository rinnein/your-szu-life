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
import { SampleField } from "./SampleField";
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

  // Scrolling textarea syncs overlay scroll position
  fireEvent.scroll(field, { target: { scrollTop: 50, scrollLeft: 20 } });
  expect(overlay.scrollTop).toBe(50);
  expect(overlay.scrollLeft).toBe(20);

  // Toggle off unmounts overlay
  fireEvent.click(screen.getByRole("button", { name: "隐藏样例 1 输出空格" }));
  expect(screen.queryByTestId("whitespace-overlay")).toBeNull();
  expect(field).toHaveValue(value);
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
it("resumes a pending result, polls sequentially until AC, then stops without a POST", async () => {
  vi.useFakeTimers();
  let resultCalls = 0;
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_oj_result")
      return ++resultCalls === 1 ? { ...pending, status: 7 } : finished;
    return { records: [pending], uncertain: [] };
  });
  const hook = renderHook(() => useSubmissions("site", "12", paper, p));
  await act(async () => {});
  expect(resultCalls).toBe(1);
  expect(hook.result.current.polling).toBe(true);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(2000);
  });
  expect(hook.result.current.result?.status).toBe(0);
  expect(hook.result.current.verdict(p)).toBe(0);
  expect(hook.result.current.polling).toBe(false);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(10000);
  });
  expect(resultCalls).toBe(2);
  expect(vi.mocked(invoke).mock.calls.every(([command]) => command.startsWith("get_"))).toBe(true);
});
it("cancels polling on a problem change and on unmount", async () => {
  vi.useFakeTimers();
  vi.mocked(invoke).mockImplementation(async (command) =>
    command === "get_oj_result" ? pending : { records: [], uncertain: [] },
  );
  const hook = renderHook(({ problem }) => useSubmissions("site", "12", paper, problem), {
    initialProps: { problem: p },
  });
  await act(async () => {});
  await act(async () => hook.result.current.show(pending));
  hook.rerender({ problem: second });
  await act(async () => {
    await vi.advanceTimersByTimeAsync(10000);
  });
  expect(hook.result.current.result).toBeUndefined();
  const calls = vi.mocked(invoke).mock.calls.length;
  hook.unmount();
  await vi.advanceTimersByTimeAsync(10000);
  expect(invoke).toHaveBeenCalledTimes(calls);
});
it("pauses after three read failures and allows an explicit retry", async () => {
  vi.useFakeTimers();
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_oj_result") throw new Error("network");
    return { records: [], uncertain: [] };
  });
  const hook = renderHook(() => useSubmissions("site", "12", paper, p));
  await act(async () => {});
  await act(async () => hook.result.current.show(pending));
  await act(async () => {
    await vi.advanceTimersByTimeAsync(12000);
  });
  expect(hook.result.current.polling).toBe(false);
  expect(hook.result.current.pollError).toContain("请手动刷新");
  vi.mocked(invoke).mockResolvedValue(finished);
  await act(async () => hook.result.current.show(pending));
  expect(hook.result.current.result?.status).toBe(0);
});
it("receives CPH results only for the current account generation", async () => {
  const hook = renderHook(() => useSubmissions("site", "12", paper, p));
  await act(async () => {});
  const callback = vi
    .mocked(listen)
    .mock.calls.find(([event]) => event === "oj-companion-result")![1];
  vi.mocked(invoke).mockResolvedValue(finished);
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
  expect(hook.result.current.result?.status).toBe(0);
});
