/** @vitest-environment jsdom */

import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vite-plus/test";
import { usePending } from "./use-pending";
import { errorMessage } from "./async-state";

describe("usePending", () => {
  it("ignores a second run with the same key while the first is in flight", async () => {
    let release: (() => void) | undefined;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const { result } = renderHook(() => usePending());
    // act 会等待传入的 promise 落地；挂起的操作直接在 act 外触发，
    // 状态断言交给 waitFor。
    const first = result.current.run("task:1", () => gate.then(() => "done"));
    await waitFor(() => expect(result.current.isPending("task:1")).toBe(true));
    const second = await act(() => result.current.run("task:1", async () => "other"));
    expect(second).toEqual({ ok: false, skipped: true });
    release?.();
    expect(await first).toEqual({ ok: true, value: "done" });
    await waitFor(() => expect(result.current.isPending("task:1")).toBe(false));
  });

  it("does not unlock unrelated keys", async () => {
    const { result } = renderHook(() => usePending());
    let releaseA: (() => void) | undefined;
    const gate = new Promise<void>((resolve) => {
      releaseA = resolve;
    });
    void result.current.run("a", () => gate);
    await waitFor(() => expect(result.current.isPending("a")).toBe(true));
    const done = await act(() => result.current.run("b", async () => 1));
    expect(done).toEqual({ ok: true, value: 1 });
    expect(result.current.isPending("a")).toBe(true);
    releaseA?.();
    await waitFor(() => expect(result.current.isPending("a")).toBe(false));
  });

  it("reports failures without throwing", async () => {
    const { result } = renderHook(() => usePending());
    const outcome = await act(() =>
      result.current.run("x", async () => {
        throw "网络错误";
      }),
    );
    expect(outcome.ok).toBe(false);
    expect((outcome as { error?: unknown }).error).toBe("网络错误");
    expect(result.current.isPending("x")).toBe(false);
  });
});

describe("errorMessage", () => {
  it("reads tauri string errors and Error.message, else falls back", () => {
    expect(errorMessage("磁盘已满")).toBe("磁盘已满");
    expect(errorMessage(new Error("timeout"))).toBe("timeout");
    expect(errorMessage({ message: "boom" })).toBe("boom");
    expect(errorMessage(undefined, "回退文案")).toBe("回退文案");
    // 空字符串同样回落；不传 fallback 时回落到默认文案。
    expect(errorMessage("", "回退文案")).toBe("回退文案");
    expect(errorMessage("")).toBe("操作未完成，请重试。");
  });
});
