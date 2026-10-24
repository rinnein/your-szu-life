/** @vitest-environment jsdom */
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEnrollment } from "./use-enrollment";
import { snapshotFixture } from "./testing/fixtures";
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class MockChannel {
    onmessage = null;
  },
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
const deferred = () => {
  let resolve!: (v: unknown) => void;
  let reject!: (v: unknown) => void;
  const promise = new Promise((a, b) => {
    resolve = a;
    reject = b;
  });
  return { promise, resolve, reject };
};
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(listen).mockResolvedValue(() => {});
  vi.mocked(invoke).mockResolvedValue(structuredClone(snapshotFixture));
});
afterEach(cleanup);
it("keeps overlapping operation errors and loading independent", async () => {
  const probe = deferred(),
    refresh = deferred();
  vi.mocked(invoke).mockImplementation(async (_, args) => {
    const type = ((args as Record<string, unknown>).action as { type: string }).type;
    return type === "probe"
      ? probe.promise
      : type === "refresh"
        ? refresh.promise
        : structuredClone(snapshotFixture);
  });
  const hook = renderHook(() => useEnrollment());
  await waitFor(() => expect(hook.result.current.snapshot).toBeDefined());
  act(() => {
    void hook.result.current.run({ type: "probe" });
    void hook.result.current.run({ type: "refresh" });
  });
  await act(async () => probe.reject("测速暂时失败"));
  expect(hook.result.current.errors.probe).toBe("测速暂时失败");
  expect(hook.result.current.pending.refresh).toBe(1);
  await act(async () => refresh.resolve(structuredClone(snapshotFixture)));
  expect(hook.result.current.errors.probe).toBe("测速暂时失败");
  expect(hook.result.current.errors.refresh).toBeFalsy();
  expect(hook.result.current.busy).toBe(false);
});
it("does not replace a newer error with a late error from the same operation", async () => {
  const old = deferred(),
    next = deferred();
  let calls = 0;
  vi.mocked(invoke).mockImplementation(async (_, args) => {
    if (((args as Record<string, unknown>).action as { type: string }).type === "probe")
      return ++calls === 1 ? old.promise : next.promise;
    return structuredClone(snapshotFixture);
  });
  const hook = renderHook(() => useEnrollment());
  await waitFor(() => expect(hook.result.current.pending.snapshot ?? 0).toBe(0));
  act(() => {
    void hook.result.current.run({ type: "probe" });
    void hook.result.current.run({ type: "probe" });
  });
  await act(async () => next.reject("新的结果"));
  await act(async () => old.reject("过时的结果"));
  expect(hook.result.current.errors.probe).toBe("新的结果");
});
it("applies a later session result even if the initial snapshot resolves first", async () => {
  const initial = deferred(),
    session = deferred();
  vi.mocked(invoke).mockImplementation(async (_, args) =>
    ((args as Record<string, unknown>).action as { type: string }).type === "session"
      ? session.promise
      : initial.promise,
  );
  const hook = renderHook(() => useEnrollment());
  act(() => {
    void hook.result.current.run({ type: "session" });
  });
  await act(async () => initial.resolve({ ...snapshotFixture, loggedIn: false }));
  await act(async () =>
    session.resolve({ status: "ready", snapshot: { ...snapshotFixture, loggedIn: true } }),
  );
  expect(hook.result.current.snapshot?.loggedIn).toBe(true);
});
