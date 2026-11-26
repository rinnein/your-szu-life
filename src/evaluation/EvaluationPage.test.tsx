/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import EvaluationPage from "./EvaluationPage";
import { mockEvaluationOverview, mockEvaluationRunOutcome } from "../demo/fixtures/evaluation";
import { resetToasts } from "../ui";
import { typeInto } from "../ui/testing";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
  Channel: class MockChannel {
    onmessage: ((message: unknown) => void) | null = null;
  },
}));

vi.mock("../logical-navigation", () => ({
  nativeRuntime: () => false,
  isChildDocument: () => false,
  useLogicalNavigation: () => ({
    push: vi.fn(),
    replace: vi.fn(),
    back: vi.fn(),
    canBack: true,
  }),
}));

function toastMessage(): string | null {
  return document.querySelector("m-snackbar")?.getAttribute("message") ?? null;
}

describe("EvaluationPage", () => {
  beforeEach(() => {
    resetToasts();
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "evaluation_overview") return mockEvaluationOverview;
      if (command === "evaluation_run") return mockEvaluationRunOutcome;
      if (command === "evaluation_save_config") return { comment: "无" };
      if (command === "refresh_todos") return undefined;
      return undefined;
    });
  });

  afterEach(() => {
    cleanup();
    resetToasts();
  });

  it("展示待评统计与目标清单", async () => {
    render(<EvaluationPage />);
    await waitFor(() =>
      expect(screen.getAllByText("常规课学生过程性评价指标-3").length).toBeGreaterThan(0),
    );
    expect(screen.getByText("待评目标")).toBeInTheDocument();
    expect(screen.getByText("待评教师")).toBeInTheDocument();
    expect(screen.getByText(/演示教师甲/)).toBeInTheDocument();
    expect(screen.getByText(/演示教师乙/)).toBeInTheDocument();
    expect(screen.getByText("已完成")).toBeInTheDocument();
  });

  it("确认后提交评教、刷新待办并提示结果", async () => {
    render(<EvaluationPage />);
    await waitFor(() =>
      expect(screen.getAllByText("常规课学生过程性评价指标-3").length).toBeGreaterThan(0),
    );

    fireEvent.click(screen.getByRole("button", { name: "开始评教" }));
    expect(await screen.findByText("确认一键评教")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "确认提交" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "evaluation_run",
        expect.objectContaining({ comment: "无", channel: expect.anything() }),
      ),
    );
    await waitFor(() => expect(toastMessage()).toContain("评教已完成"));
    expect(invokeMock).toHaveBeenCalledWith("refresh_todos");
    // 提交后回读概览（走缓存，不强制刷新）。
    const overviewCalls = invokeMock.mock.calls.filter(
      ([command]) => command === "evaluation_overview",
    );
    expect(overviewCalls.length).toBeGreaterThanOrEqual(2);
  });

  it("保存开放题回答", async () => {
    render(<EvaluationPage />);
    await waitFor(() =>
      expect(screen.getAllByText("常规课学生过程性评价指标-3").length).toBeGreaterThan(0),
    );

    typeInto(screen.getByLabelText("开放题回答"), "好课");
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("evaluation_save_config", { comment: "好课" }),
    );
    await waitFor(() => expect(toastMessage()).toContain("已保存开放题回答"));
  });

  it("一键评教卡片位于评教设置之上，清单用分隔线隔开", async () => {
    render(<EvaluationPage />);
    await waitFor(() =>
      expect(screen.getAllByText("常规课学生过程性评价指标-3").length).toBeGreaterThan(0),
    );

    const run = screen.getByText("一键评教");
    const settings = screen.getByText("评教设置");
    expect(run.compareDocumentPosition(settings) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    const list = screen.getByRole("region", { name: "评价清单" });
    const divider = document.querySelector("m-divider");
    expect(divider).not.toBeNull();
    expect(divider!.compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(
      settings.compareDocumentPosition(divider!) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("加载失败时显示可重试的 Banner", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "evaluation_overview") {
        throw new Error("统一认证会话已失效，请重新登录应用");
      }
      return undefined;
    });
    render(<EvaluationPage />);
    await waitFor(() => expect(screen.getByText("评教数据加载失败")).toBeInTheDocument());
    expect(screen.getByText(/统一认证会话已失效/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "开始评教" })).toBeDisabled();
  });
});
