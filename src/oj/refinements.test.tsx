/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { ojProgressCache } from "./progress-store";
import { ContestTable } from "./ContestTable";
import { RankingTable } from "./RankingTable";
import { SubmissionRecord } from "./SubmissionRecord";
import { Verdict } from "./JudgeResult";
import { verdicts } from "./judging";
import type { Contest, Paper } from "./types";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const contest: Contest = {
  id: "1",
  groupId: "1",
  title: "练习",
  description: "",
  begin: 0,
  end: 300,
  now: 200,
  isExam: false,
};
const ended: Contest = { ...contest, id: "2", end: 100 };
const paper = {
  contest,
  account: "fixture",
  epoch: 1,
  problems: [{ id: "1", displayLabel: "A", kind: 0 }],
} as Paper;
beforeEach(() => {
  ojProgressCache.clear();
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
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
it.each(Object.entries(verdicts))("gives status %s a filled semantic label", (status, verdict) => {
  render(<Verdict status={Number(status)} />);
  const chip = screen.getByLabelText(`${verdict.code} · ${verdict.label}`);
  // M3 的 Chip 用 data 属性表达变体与配色，而不是 BEM 类名。
  expect(chip).toHaveAttribute("data-variant", "primary");
  expect(chip).toHaveAttribute("data-color", verdict.color);
});
it("marks a running contest complete only when all problems are AC, and opens on row activation", async () => {
  vi.mocked(invoke).mockResolvedValue({ accepted: 2, total: 2 });
  const open = vi.fn();
  const view = render(
    <ContestTable contests={[contest]} query="" siteId="test" account="fixture" onOpen={open} />,
  );
  // 「已完成」写在 overline 上，跨行可读。
  expect(await screen.findByText("已完成")).toBeInTheDocument();
  expect(screen.getByText(/进行中/)).toBeInTheDocument();
  // MD3 List 的行是一次激活（整行可点），不再有单独的「进入测验」按钮。
  fireEvent.click(screen.getAllByRole("listitem")[0]);
  expect(open).toHaveBeenCalledWith(contest);
  view.unmount();
  const openEnded = vi.fn();
  render(
    <ContestTable contests={[ended]} query="" siteId="test" account="fixture" onOpen={openEnded} />,
  );
  expect(screen.getByText(/已结束/)).toBeInTheDocument();
  // 已结束的行尾部显示「查看测验」，与进行中的「进入测验」区分。
  expect(screen.getByText("查看测验")).toBeInTheDocument();
  fireEvent.click(screen.getAllByRole("listitem")[0]);
  expect(openEnded).toHaveBeenCalledWith(ended);
});
it("does not fetch completion for contests that are not running", async () => {
  vi.mocked(invoke).mockResolvedValue({ accepted: 1, total: 2 });
  render(
    <ContestTable
      contests={[ended, { ...contest, id: "3", begin: 250 }, contest]}
      query=""
      siteId="test"
      account="fixture"
      onOpen={() => {}}
    />,
  );
  await waitFor(() => expect(invoke).toHaveBeenCalledTimes(1));
  expect(invoke).toHaveBeenCalledWith("get_oj_progress", {
    siteId: "test",
    contestId: "1",
    account: "fixture",
    sessionGeneration: null,
    force: false,
  });
  expect(screen.queryByText("已完成")).toBeNull();
});
it.each([
  { accepted: 0, total: 0 },
  { accepted: 1, total: 2 },
])("does not mark incomplete or empty contests complete: %s", async (progress) => {
  vi.mocked(invoke).mockResolvedValue(progress);
  render(
    <ContestTable
      contests={[contest]}
      query=""
      siteId="test"
      account="fixture"
      onOpen={() => {}}
    />,
  );
  await waitFor(() => expect(invoke).toHaveBeenCalled());
  expect(screen.queryByText("已完成")).toBeNull();
});
it("keeps code and results collapsed and mounts a read-only CodeMirror only when expanded", async () => {
  const show = vi.fn();
  render(
    <SubmissionRecord
      submission={{ id: 12, problem: 1, language: "cpp", code: "  int main(){}\n", status: 0 }}
      desktop={false}
      polling={false}
      onShow={show}
    />,
  );
  expect(screen.queryByLabelText("提交 12 代码")).toBeNull();
  expect(screen.queryByLabelText("提交 12 判题结果")).toBeNull();
  expect(screen.getByLabelText("代码语言")).toHaveTextContent("C++");
  fireEvent.click(screen.getByRole("button", { name: "查看代码" }));
  const editor = await screen.findByLabelText("提交 12 代码");
  expect(editor).toHaveAttribute("contenteditable", "false");
  expect(editor).toHaveTextContent("int main()");
  fireEvent.click(screen.getByRole("button", { name: "查看代码" }));
  expect(screen.queryByLabelText("提交 12 代码")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "查看结果" }));
  expect(show).toHaveBeenCalledOnce();
  expect(await screen.findByLabelText("提交 12 判题结果")).toBeVisible();
});
it("loads and paginates rankings only after opening the modal", async () => {
  vi.mocked(invoke).mockResolvedValue({
    count: 21,
    rows: [
      {
        rank: 1,
        username: "fixture",
        name: "测试",
        nickname: "",
        accepted: 1,
        penalty: "00:01:20",
        problems: { "1": { accepted: true, errors: 0, seconds: 80 } },
      },
    ],
  });
  render(<RankingTable siteId="test" paper={paper} />);
  expect(invoke).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "排名" }));
  expect(await screen.findByRole("grid", { name: "测验内排名" })).toHaveTextContent("fixture");
  expect(screen.getByText("我")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "下一页" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("get_oj_ranking", {
      siteId: "test",
      contestId: "1",
      offset: 20,
    }),
  );
});
it("keeps a server ranking denial as an inline error", async () => {
  vi.mocked(invoke).mockRejectedValue(new Error("此测验未开放排名"));
  render(<RankingTable siteId="test" paper={paper} />);
  fireEvent.click(screen.getByRole("button", { name: "排名" }));
  expect(await screen.findByText(/此测验未开放排名/)).toBeInTheDocument();
  expect(screen.queryByRole("grid")).toBeNull();
});
