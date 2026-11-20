/** @vitest-environment jsdom */
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { renderHook, waitFor } from "@testing-library/react";
import {
  TA_LIST_TTL_MS,
  clearTaCache,
  invalidateTaReviews,
  readTaCache,
  taReviewsKey,
  writeTaCache,
} from "./ta-cache";
import { useTaList } from "./useTaList";

let now = 1_000;
beforeEach(() => {
  clearTaCache();
  now = 1_000;
  vi.spyOn(Date, "now").mockImplementation(() => now);
});
afterEach(() => {
  vi.restoreAllMocks();
  clearTaCache();
});

it("expires TA list entries after the TTL", () => {
  const key = taReviewsKey("site", "account", 1, "12");
  expect(readTaCache(key)).toBeUndefined();
  writeTaCache(key, { count: 0, rows: [] });
  expect(readTaCache(key)?.fresh).toBe(true);
  now += TA_LIST_TTL_MS;
  expect(readTaCache(key)?.fresh).toBe(false);
});

it("invalidates every page of one contest but not another", () => {
  const first = taReviewsKey("site", "account", 1, "12", "a:0:20");
  const second = taReviewsKey("site", "account", 1, "12", "b:20:20");
  const other = taReviewsKey("site", "account", 1, "13", "a:0:20");
  writeTaCache(first, { count: 1 });
  writeTaCache(second, { count: 1 });
  writeTaCache(other, { count: 1 });
  invalidateTaReviews("site", "account", 1, "12");
  expect(readTaCache(first)).toBeUndefined();
  expect(readTaCache(second)).toBeUndefined();
  expect(readTaCache(other)).toBeDefined();
});

it("does not invoke again while the cache is fresh", async () => {
  const load = vi.fn(async () => ({ count: 0, rows: [] }));
  const first = renderHook(() => useTaList({ cacheKey: "reviews:site:account:1:12", load }));
  await waitFor(() => expect(first.result.current.state.status).toBe("ready"));
  expect(load).toHaveBeenCalledTimes(1);
  first.unmount();

  const second = renderHook(() => useTaList({ cacheKey: "reviews:site:account:1:12", load }));
  await waitFor(() => expect(second.result.current.state.status).toBe("ready"));
  expect(load).toHaveBeenCalledTimes(1);
  second.unmount();

  now += TA_LIST_TTL_MS;
  const third = renderHook(() => useTaList({ cacheKey: "reviews:site:account:1:12", load }));
  await waitFor(() => expect(third.result.current.state.status).toBe("ready"));
  expect(load).toHaveBeenCalledTimes(2);
  third.unmount();
});

it("manual refresh always invokes and overwrites the cache", async () => {
  const load = vi.fn(async () => ({ count: 1, rows: [] }));
  const hook = renderHook(() => useTaList({ cacheKey: "reviews:site:account:1:12", load }));
  await waitFor(() => expect(hook.result.current.state.status).toBe("ready"));
  expect(load).toHaveBeenCalledTimes(1);
  await hook.result.current.refresh();
  expect(load).toHaveBeenCalledTimes(2);
  hook.unmount();
});
