/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { ContestTable, formatRemaining } from "./ContestTable";
import { mockElementRects } from "../test-layout";
import type { Contest } from "./types";
const now = Date.parse("2026-10-04T12:00:00Z");
const contest: Contest = {
  id: "1",
  groupId: "2",
  title: "测试测验",
  description: "",
  begin: now - 1000,
  end: now + 2000,
  now,
  isExam: false,
};
beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(10000);
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
  vi.useRealTimers();
  vi.unstubAllGlobals();
});
/**
 * 进行中的测验由 List 的选中态表达；行元素在 shadow root 里，
 * 因此断言宿主的 `value` 与内部行的选中类。
 */
function selectedRow(container: HTMLElement): boolean {
  const host = container.querySelector("m-list");
  return !!host?.shadowRoot?.querySelector(".mtrl-list__item--selected");
}

/**
 * List 由 MutationObserver 在微任务里刷新行内容；测试用假时钟，
 * `waitFor` 会因时钟不推进而挂起，因此手动把微任务跑完。
 */
async function flushList(): Promise<void> {
  await act(async () => {
    await Promise.resolve();
  });
}

it("highlights the running contest and counts down using the server clock", async () => {
  const view = render(<ContestTable contests={[contest]} query="" onOpen={() => {}} />);
  expect(selectedRow(view.container)).toBe(true);
  expect(screen.getByText(/剩余 00:00:02/)).toBeTruthy();
  act(() => vi.advanceTimersByTime(1000));
  await flushList();
  expect(screen.getByText(/剩余 00:00:01/)).toBeTruthy();
  act(() => vi.advanceTimersByTime(1000));
  await flushList();
  // 结束后不再高亮，也不再显示倒计时。
  expect(selectedRow(view.container)).toBe(false);
  expect(screen.getByText(/已结束/)).toBeTruthy();
  expect(screen.queryByText(/剩余/)).toBeNull();
});
it("starts highlighting and counting down when a future contest begins", async () => {
  const future = { ...contest, begin: now + 1000, end: now + 3000 };
  const view = render(<ContestTable contests={[future]} query="" onOpen={() => {}} />);
  expect(screen.getByText("未开始")).toBeTruthy();
  expect(screen.queryByText(/剩余/)).toBeNull();
  act(() => vi.advanceTimersByTime(1000));
  await flushList();
  expect(selectedRow(view.container)).toBe(true);
  expect(screen.getByText(/剩余 00:00:02/)).toBeTruthy();
});
it("formats long and expired durations without negative countdowns", () => {
  expect(formatRemaining(90061000)).toBe("1天 01:01:01");
  expect(formatRemaining(-1000)).toBe("00:00:00");
  expect(formatRemaining(1)).toBe("00:00:01");
});

it("supports filtering contests by status tabs", async () => {
  mockElementRects({ "adaptive-tabs": 800 });
  const endedContest = {
    ...contest,
    id: "2",
    title: "已结束测验",
    begin: now - 5000,
    end: now - 1000,
  };
  const view = render(
    <ContestTable contests={[contest, endedContest]} query="" onOpen={() => {}} />,
  );

  // 默认全部测验展示
  expect(screen.getByText("全部")).toBeTruthy();
  const endedTab = screen.getByRole("tab", { name: /已结束/ });
  act(() => {
    endedTab.click();
  });
  await flushList();

  // 选中已结束后，进行中的测验被过滤掉
  const host = view.container.querySelector("m-list");
  expect(host?.shadowRoot?.textContent).not.toContain("测试测验");
  expect(host?.shadowRoot?.textContent).toContain("已结束测验");
});
