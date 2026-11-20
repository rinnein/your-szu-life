/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { TaProblemView } from "./TaProblemView";
import type { ManagedContestDetail, Problem, Site } from "../types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const site: Site = {
  id: "test",
  name: "测试 OJ",
  url: "http://oj.test/",
  builtin: false,
  reminders: true,
  enabled: true,
  username: "assistant",
  session: "ready",
  message: "",
  permission: 2,
};

const coding: Problem = {
  id: "1",
  displayLabel: "A",
  kind: 0,
  title: "相加",
  description: "<p>计算</p>",
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
  score: 20,
  ioMode: "stdio",
};
const choice: Problem = {
  ...coding,
  id: "2",
  displayLabel: "A",
  kind: 1,
  title: "选择",
  description: "选一个",
  choices: ["甲", "乙"],
};
const contest: ManagedContestDetail = {
  id: "12",
  groupId: "3",
  groupName: "班级",
  title: "测验",
  description: "",
  begin: Date.now() - 10_000,
  end: Date.now() + 3_600_000,
  isExam: false,
  scoredStatus: "done",
  problems: [coding, choice],
  now: Date.now(),
};

beforeEach(() => {
  vi.mocked(invoke).mockResolvedValue({ count: 0, rows: [] });
  window.matchMedia = vi.fn().mockImplementation((media) => ({
    media,
    matches: false,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn(),
  }));
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
  vi.clearAllMocks();
  vi.unstubAllGlobals();
});

it("defaults programming problems to the student submission list", async () => {
  render(
    <TaProblemView
      site={site}
      contest={contest}
      problem="0:1"
      desktop={false}
      onSelectProblem={() => {}}
      onSelectTab={() => {}}
      onOpenSubmission={() => {}}
      onOpenStudent={() => {}}
    />,
  );
  expect(await screen.findByText(/相加/)).toBeTruthy();
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith(
      "get_oj_problem_submissions",
      expect.objectContaining({ problemId: "1", kind: 0 }),
    ),
  );
});

it("defaults non-programming problems to the statement instead of an empty submission table", async () => {
  render(
    <TaProblemView
      site={site}
      contest={contest}
      problem="1:2"
      desktop={false}
      onSelectProblem={() => {}}
      onSelectTab={() => {}}
      onOpenSubmission={() => {}}
      onOpenStudent={() => {}}
    />,
  );
  expect(await screen.findByText(/选一个/)).toBeTruthy();
  expect(invoke).not.toHaveBeenCalledWith("get_oj_problem_submissions", expect.anything());
});
