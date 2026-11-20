/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { clearTaCache } from "./ta-cache";
import { TaReviewDetail } from "./TaReviewDetail";
import type { Site } from "../types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../CodeEditor", () => ({
  CodeEditor: ({ value }: { value: string }) => <pre>{value}</pre>,
}));

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

const detail = {
  contest: {
    id: "12",
    groupId: "3",
    groupName: "班级",
    title: "测验",
    description: "",
    begin: Date.now() - 10_000,
    end: Date.now() + 3_600_000,
    isExam: false,
    scoredStatus: "done",
  },
  submissionId: "77",
  user: { id: "9", username: "s1", name: "学生", nickname: "" },
  totalScore: 0,
  judged: true,
  checked: false,
  problems: [
    {
      id: "1",
      displayLabel: "A",
      kind: 0,
      title: "相加",
      description: "<p>计算</p>",
      hint: "",
      maxScore: 20,
      score: 0,
      submissionId: "88",
      answer: null,
      choices: [],
      multiple: false,
      blanks: 0,
      code: "int main(){}",
      language: "cpp",
      status: 0,
      comment: "",
      referenceAnswer: "int main(){return 0;}",
      judge: null,
      loadError: null,
    },
  ],
  warnings: [],
};

beforeEach(() => {
  clearTaCache();
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_oj_review_detail") return structuredClone(detail);
    if (command === "save_oj_review")
      return {
        saved: true,
        uncertain: false,
        message: "",
        detail: structuredClone(detail),
      };
    return null;
  });
});

afterEach(cleanup);

it("confirms before saving scores and marks the answer reviewed", async () => {
  render(<TaReviewDetail site={site} contestId="12" submissionId="77" desktop={false} />);
  const save = await screen.findByRole("button", { name: "保存并标记已阅" });
  fireEvent.click(save);
  const confirm = await screen.findByRole("button", { name: "确认保存" });
  fireEvent.click(confirm);
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith(
      "save_oj_review",
      expect.objectContaining({
        input: expect.objectContaining({
          siteId: "test",
          contestId: "12",
          submissionId: "77",
          scores: expect.objectContaining({
            coding: { "1": { "88": 0 } },
            codeFill: {},
            qa: {},
            choice: {},
            fillIn: {},
          }),
        }),
      }),
    ),
  );
});
