/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, renderHook } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vite-plus/test";
import {
  currentPageBackHandler,
  setPageBackHandler,
  subscribePageBackHandler,
  usePageBackHandler,
} from "./page-back";

afterEach(() => {
  cleanup();
  setPageBackHandler(undefined);
});

it("runs the registered handler and notifies subscribers", async () => {
  const handler = vi.fn();
  const listener = vi.fn();
  const unsubscribe = subscribePageBackHandler(listener);
  setPageBackHandler(handler);
  expect(currentPageBackHandler()).toBe(handler);
  await currentPageBackHandler()?.();
  expect(handler).toHaveBeenCalledOnce();
  expect(listener).toHaveBeenCalledOnce();
  unsubscribe();
});

it("registers through the hook and clears on unmount", () => {
  const handler = vi.fn();
  const view = renderHook(() => usePageBackHandler(handler));
  expect(currentPageBackHandler()).toBe(handler);
  view.unmount();
  expect(currentPageBackHandler()).toBeUndefined();
});

it("does not clear a newer handler when an older registration unmounts", () => {
  const first = vi.fn();
  const second = vi.fn();
  const older = renderHook(() => usePageBackHandler(first));
  const newer = renderHook(() => usePageBackHandler(second));
  expect(currentPageBackHandler()).toBe(second);
  older.unmount();
  expect(currentPageBackHandler()).toBe(second);
  newer.unmount();
  expect(currentPageBackHandler()).toBeUndefined();
});
