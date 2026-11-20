/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { cleanup, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { clearTaCache } from "./ta-cache";
import { TaReviewList } from "./TaReviewList";
import type { Site } from "../types";

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

beforeEach(() => {
  clearTaCache();
  vi.mocked(invoke).mockResolvedValue({
    count: 3,
    rows: [
      {
        id: "1",
        user: { id: "1", username: "s1", name: "学生一", nickname: "" },
        totalScore: 10,
        judged: true,
        checked: true,
      },
      {
        id: "2",
        user: { id: "2", username: "s2", name: "学生二", nickname: "" },
        totalScore: 8,
        judged: true,
        checked: false,
      },
      {
        id: "3",
        user: { id: "3", username: "s3", name: "学生三", nickname: "" },
        totalScore: 0,
        judged: false,
        checked: false,
      },
    ],
  });
});

afterEach(cleanup);

it("labels reviewed, unreviewed and unjudged students", async () => {
  render(<TaReviewList site={site} contestId="12" onOpenReview={() => {}} />);
  await screen.findByText("s1");
  expect(screen.getByText("已阅")).toBeTruthy();
  expect(screen.getByText("未人工批阅")).toBeTruthy();
  expect(screen.getByText("尚未自动评分")).toBeTruthy();
});
