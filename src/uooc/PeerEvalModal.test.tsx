/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { PeerEvalModal } from "./PeerEvalModal";
import type { PeerEvalTask, UoocCourse } from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const mockCourses: UoocCourse[] = [
  {
    id: "101",
    title: "计算机网络",
    semester: "2026秋",
    organization: "深圳大学",
    weeks: "1-16周",
  },
];

const mockEvalTasks: PeerEvalTask[] = [
  {
    cid: "101",
    tid: "202",
    title: "第一章互评",
    courseName: "计算机网络",
    startTime: "2026-10-01 00:00:00",
    endTime: "2026-10-25 00:00:00",
    totalCount: 1,
    completedCount: 0,
    submissions: [
      {
        evalId: "sub_1",
        studentTaskId: "202",
        answers: {},
        content: "TCP三次握手详细过程描述...",
        score: null,
        comments: null,
        isEvaluated: false,
      },
    ],
  },
];

describe("PeerEvalModal", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it("loads and displays peer submissions for evaluation", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === "get_uooc_eval_list") {
        return mockEvalTasks;
      }
      return null;
    });

    render(
      <PeerEvalModal
        isOpen={true}
        onClose={vi.fn()}
        courses={mockCourses}
        defaultCid="101"
      />,
    );

    await waitFor(() => {
      expect(screen.getByText("第一章互评")).toBeInTheDocument();
      expect(screen.getByText(/TCP三次握手详细过程描述/)).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /提交此篇评分/ })).toBeInTheDocument();
    });
  });

  it("submits single evaluation successfully", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === "get_uooc_eval_list") {
        return mockEvalTasks;
      }
      if (cmd === "submit_uooc_eval") {
        return "互评提交成功";
      }
      return null;
    });

    render(
      <PeerEvalModal
        isOpen={true}
        onClose={vi.fn()}
        courses={mockCourses}
        defaultCid="101"
      />,
    );

    await waitFor(() => {
      expect(screen.getByRole("button", { name: /提交此篇评分/ })).toBeInTheDocument();
    });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /提交此篇评分/ }));
    });

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("submit_uooc_eval", expect.objectContaining({
        cid: "101",
        evalId: "sub_1",
      }));
    });
  });
});
