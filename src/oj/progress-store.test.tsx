/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import {
  createProgressCache,
  ojProgressCache,
  progressScopeKey,
  PROGRESS_TTL,
  type ProgressScope,
} from "./progress-store";
import { ContestTable } from "./ContestTable";
import type { ProgressSummary } from "./types";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const scope: ProgressScope = {
  siteId: "site",
  account: "student",
  url: "http://oj.test/",
  generation: 1,
};
const complete = { accepted: 2, total: 2 };
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}
beforeEach(() => {
  vi.clearAllMocks();
  ojProgressCache.clear();
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
  vi.unstubAllGlobals();
});
it("coalesces callers and reuses successful state on another page", async () => {
  const response = deferred<ProgressSummary>();
  const fetch = vi.fn(() => response.promise);
  const cache = createProgressCache(fetch, () => 1000);
  const first = cache.ensure(scope, "1");
  const second = cache.ensure(scope, "1");
  expect(first).toBe(second);
  response.resolve(complete);
  await first;
  await cache.ensure(scope, "1");
  expect(fetch).toHaveBeenCalledTimes(1);
  expect(cache.store.get()[progressScopeKey(scope)].entries["1"].value).toEqual(complete);
});
it("refreshes expired entries, keeps the last value on failure and backs off", async () => {
  let now = 1000;
  const fetch = vi.fn().mockResolvedValue(complete);
  const cache = createProgressCache(fetch, () => now);
  await cache.ensure(scope, "1");
  now += PROGRESS_TTL;
  fetch.mockRejectedValue(new Error("offline"));
  await cache.ensure(scope, "1");
  await cache.ensure(scope, "1");
  expect(fetch).toHaveBeenCalledTimes(2);
  expect(cache.store.get()[progressScopeKey(scope)].entries["1"].value).toEqual(complete);
  now += 30_000;
  fetch.mockResolvedValue({ accepted: 1, total: 2 });
  await cache.ensure(scope, "1");
  expect(cache.store.get()[progressScopeKey(scope)].entries["1"].value).toEqual({
    accepted: 1,
    total: 2,
  });
});
it("manual refresh bypasses both freshness and failure backoff, only for selected contests", async () => {
  const fetch = vi.fn().mockResolvedValue(complete);
  const cache = createProgressCache(fetch, () => 0);
  await cache.ensure(scope, "1");
  await cache.ensure(scope, "2");
  cache.invalidate(scope, ["1"], true);
  await cache.ensure(scope, "1");
  await cache.ensure(scope, "2");
  expect(fetch).toHaveBeenCalledTimes(3);
  expect(fetch).toHaveBeenLastCalledWith(scope, "1", true);
});
it("cannot let old requests overwrite a newer result derived from the paper", async () => {
  const response = deferred<ProgressSummary>();
  const cache = createProgressCache(
    () => response.promise,
    () => 1000,
  );
  const pending = cache.ensure(scope, "1");
  cache.seed(scope, "1", complete);
  response.resolve({ accepted: 0, total: 2 });
  await pending;
  expect(cache.store.get()[progressScopeKey(scope)].entries["1"].value).toEqual(complete);
});
it("isolates sites, URLs, accounts and credential generations", async () => {
  const fetch = vi.fn().mockResolvedValue(complete);
  const cache = createProgressCache(fetch, () => 1000);
  for (const s of [
    scope,
    { ...scope, siteId: "other" },
    { ...scope, url: "http://new.test/" },
    { ...scope, account: "other" },
    { ...scope, generation: 2 },
  ])
    await cache.ensure(s, "1");
  expect(fetch).toHaveBeenCalledTimes(5);
});
it("drops removed accounts and rejects their pending replies", async () => {
  const response = deferred<ProgressSummary>();
  const cache = createProgressCache(
    () => response.promise,
    () => 1000,
  );
  const pending = cache.ensure(scope, "1");
  cache.reconcile([]);
  response.resolve(complete);
  await pending;
  expect(cache.store.get()).toEqual({});
});
it("native invalidations target one account generation and contest", async () => {
  const fetch = vi.fn().mockResolvedValue(complete);
  const cache = createProgressCache(fetch, () => 1000);
  await cache.ensure(scope, "1");
  await cache.ensure(scope, "2");
  cache.invalidateNative({
    siteId: "site",
    account: "student",
    sessionGeneration: 0,
    contestId: "1",
  });
  await cache.ensure(scope, "1");
  expect(fetch).toHaveBeenCalledTimes(2);
  cache.invalidateNative({
    siteId: "site",
    account: "student",
    sessionGeneration: 1,
    contestId: "1",
  });
  await cache.ensure(scope, "1");
  await cache.ensure(scope, "2");
  expect(fetch).toHaveBeenCalledTimes(3);
});
it("rejects malformed counts instead of caching a completed label", async () => {
  const cache = createProgressCache(
    async () => ({ accepted: 3, total: 2 }),
    () => 1000,
  );
  await cache.ensure(scope, "1");
  expect(cache.store.get()[progressScopeKey(scope)].entries["1"].value).toBeUndefined();
});
it("reuses the completion label after remount, props refresh and window focus", async () => {
  vi.mocked(invoke).mockResolvedValue(complete);
  const contest = {
    id: "1",
    groupId: "1",
    title: "测验",
    description: "",
    begin: 0,
    end: 2000,
    now: 1000,
    isExam: false,
  };
  const props = {
    siteId: "site",
    account: "student",
    siteUrl: scope.url,
    sessionGeneration: 1,
    query: "",
    onOpen: () => {},
  };
  const first = render(<ContestTable {...props} contests={[contest]} />);
  await screen.findByText("已完成");
  first.unmount();
  const next = render(<ContestTable {...props} contests={[{ ...contest }]} />);
  expect(screen.getByText("已完成")).toBeInTheDocument();
  fireEvent.focus(window);
  fireEvent(document, new Event("visibilitychange"));
  next.rerender(<ContestTable {...props} contests={[{ ...contest }]} />);
  await act(async () => {});
  expect(invoke).toHaveBeenCalledTimes(1);
});
it("switches accounts without briefly showing another account's completed label", async () => {
  vi.mocked(invoke).mockResolvedValue(complete);
  const contest = {
    id: "1",
    groupId: "1",
    title: "测验",
    description: "",
    begin: 0,
    end: 2000,
    now: 1000,
    isExam: false,
  };
  const props = {
    siteId: "site",
    siteUrl: scope.url,
    query: "",
    onOpen: () => {},
    contests: [contest],
  };
  const view = render(<ContestTable {...props} account="first" sessionGeneration={1} />);
  await screen.findByText("已完成");
  vi.mocked(invoke).mockResolvedValue({ accepted: 0, total: 2 });
  view.rerender(<ContestTable {...props} account="second" sessionGeneration={2} />);
  // List 的行内容由 MutationObserver 在微任务里刷新，先让它跑完再断言，
  // 否则读到的是上一个账户的行。
  await act(async () => {
    await Promise.resolve();
  });
  expect(screen.queryByText("已完成")).toBeNull();
  await waitFor(() => expect(invoke).toHaveBeenCalledTimes(2));
});
