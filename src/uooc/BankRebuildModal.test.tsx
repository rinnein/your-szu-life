/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { BankRebuildModal, type BankRebuildProgress } from "./BankRebuildModal";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

describe("BankRebuildModal", () => {
  let onProgress: EventCallback<BankRebuildProgress> | undefined;

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
    vi.mocked(listen).mockImplementation(async (event, callback) => {
      if (event === "uooc-bank-rebuild-progress") {
        onProgress = callback as EventCallback<BankRebuildProgress>;
      }
      return () => {};
    });
  });

  afterEach(() => {
    cleanup();
  });

  it("renders live progress hierarchy: course (cid) -> task (tid) -> question (qid)", async () => {
    const onOpenChange = vi.fn();
    const onComplete = vi.fn();

    render(
      <BankRebuildModal
        isOpen={true}
        onOpenChange={onOpenChange}
        isBuilding={true}
        onComplete={onComplete}
      />,
    );

    expect(screen.getByText("本地题库构建流程进度")).toBeInTheDocument();
    expect(screen.getByText("正在解析收录")).toBeInTheDocument();

    // 1. 发送 course 进度事件
    await act(async () => {
      onProgress?.({
        event: "uooc-bank-rebuild-progress",
        id: 1,
        payload: {
          stage: "course",
          cid: "42",
          courseName: "高等数学",
          current: 1,
          total: 1,
          message: "正在检索课程「高等数学」(cid: 42) 的题单与测验...",
        },
      });
    });

    expect(await screen.findByText("高等数学")).toBeInTheDocument();
    expect(screen.getByText("(cid: 42)")).toBeInTheDocument();

    // 2. 发送 task 进度事件
    await act(async () => {
      onProgress?.({
        event: "uooc-bank-rebuild-progress",
        id: 2,
        payload: {
          stage: "task",
          cid: "42",
          courseName: "高等数学",
          tid: "101",
          taskName: "绪论测验",
          current: 1,
          total: 1,
          message: "正在解析测验「绪论测验」(tid: 101)...",
        },
      });
    });

    expect(await screen.findByText("绪论测验")).toBeInTheDocument();
    expect(screen.getByText("(tid: 101)")).toBeInTheDocument();

    // 3. 发送 question 进度事件
    await act(async () => {
      onProgress?.({
        event: "uooc-bank-rebuild-progress",
        id: 3,
        payload: {
          stage: "question",
          cid: "42",
          courseName: "高等数学",
          tid: "101",
          taskName: "绪论测验",
          questionId: "2057971114",
          questionStem: "微积分定理证明中正确的是",
          current: 1,
          total: 1,
          message: "已解析题目 [ID: 2057971114]",
        },
      });
    });

    expect(await screen.findByText("ID: 2057971114")).toBeInTheDocument();
    expect(screen.getByText("微积分定理证明中正确的是")).toBeInTheDocument();
    expect(screen.getByText("✓ 已收录")).toBeInTheDocument();

    // 4. 发送 complete 事件
    await act(async () => {
      onProgress?.({
        event: "uooc-bank-rebuild-progress",
        id: 4,
        payload: {
          stage: "complete",
          cid: "",
          courseName: "",
          current: 1,
          total: 1,
          message: "本地题库重构完成，共收录 1 道题目",
        },
      });
    });

    expect(await screen.findByText("构建完成")).toBeInTheDocument();
    expect(screen.getByText("共收录 1 道题目")).toBeInTheDocument();

    // 完成后点击按钮关闭
    const finishBtn = screen.getByRole("button", { name: "完成并查看题库" });
    expect(finishBtn).toBeEnabled();
    fireEvent.click(finishBtn);

    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(onComplete).toHaveBeenCalled();
  });
});
