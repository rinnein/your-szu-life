/** @vitest-environment jsdom */
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { ContestTable, formatRemaining } from "./ContestTable";
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
it("highlights the entire active row and counts down using the server clock", () => {
  const view = render(<ContestTable contests={[contest]} query="" onOpen={() => {}} />);
  expect(view.container.querySelector("tbody tr")?.classList.contains("oj-contest-active")).toBe(
    true,
  );
  expect(screen.getByText("（剩余 00:00:02）")).toBeTruthy();
  act(() => vi.advanceTimersByTime(1000));
  expect(screen.getByText("（剩余 00:00:01）")).toBeTruthy();
  act(() => vi.advanceTimersByTime(1000));
  expect(screen.getByText("已结束").closest(".chip")?.className).toContain("danger");
  expect(view.container.querySelector("tbody tr")?.classList.contains("oj-contest-active")).toBe(
    false,
  );
  expect(screen.queryByText(/剩余/)).toBeNull();
});
it("starts highlighting and counting down when a future contest begins", () => {
  const future = { ...contest, begin: now + 1000, end: now + 3000 };
  const view = render(<ContestTable contests={[future]} query="" onOpen={() => {}} />);
  expect(screen.getByText("未开始")).toBeTruthy();
  expect(screen.queryByText(/剩余/)).toBeNull();
  act(() => vi.advanceTimersByTime(1000));
  expect(view.container.querySelector("tbody tr")?.classList.contains("oj-contest-active")).toBe(
    true,
  );
  expect(screen.getByText("（剩余 00:00:02）")).toBeTruthy();
});
it("formats long and expired durations without negative countdowns", () => {
  expect(formatRemaining(90061000)).toBe("1天 01:01:01");
  expect(formatRemaining(-1000)).toBe("00:00:00");
  expect(formatRemaining(1)).toBe("00:00:01");
});
