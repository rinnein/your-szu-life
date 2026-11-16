/** @vitest-environment jsdom */

import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vite-plus/test";
import { useAsyncLoad, useLatestRequest } from "./async-state";

describe("useAsyncLoad", () => {
  it("loads once on mount and does not refetch on re-render", async () => {
    const load = vi.fn(async () => "快照");
    const { result, rerender } = renderHook(() => useAsyncLoad(load));

    await waitFor(() => expect(result.current.state).toEqual({ status: "ready", data: "快照" }));
    rerender();
    expect(load).toHaveBeenCalledTimes(1);
  });

  it("surfaces the failure and recovers on reload", async () => {
    let attempt = 0;
    const load = vi.fn(async () => {
      attempt += 1;
      if (attempt === 1) throw "读取设置失败";
      return { enabled: true };
    });
    const { result } = renderHook(() => useAsyncLoad(load));

    await waitFor(() => expect(result.current.state.status).toBe("error"));
    expect(result.current.state).toMatchObject({ status: "error", message: "读取设置失败" });

    await act(() => result.current.reload());
    expect(result.current.state).toEqual({ status: "ready", data: { enabled: true } });
    expect(load).toHaveBeenCalledTimes(2);
  });
});

describe("useLatestRequest", () => {
  it("keeps the newest response when an older request resolves later", async () => {
    const resolvers: Array<(value: string) => void> = [];
    const load = () =>
      new Promise<string>((resolve) => {
        resolvers.push(resolve);
      });
    const { result } = renderHook(() => useLatestRequest<string>(load));

    let first!: Promise<boolean>;
    let second!: Promise<boolean>;
    act(() => {
      first = result.current.run();
    });
    act(() => {
      second = result.current.run();
    });

    resolvers[1]("新结果");
    await act(async () => {
      await second;
    });
    expect(result.current.state).toEqual({ status: "ready", data: "新结果" });

    // 旧请求随后完成，必须被丢弃而不是覆盖新结果。
    resolvers[0]("旧结果");
    await act(async () => {
      await first;
    });
    expect(result.current.state).toEqual({ status: "ready", data: "新结果" });
  });
});
