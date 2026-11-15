import { useCallback, useRef, useState } from "react";

/**
 * 异步数据与请求状态的统一模型。
 *
 * UOOC 模块此前用「数据 + loading 布尔 + error 字符串」的旗子组合表达状态，
 * 出现过「错误时渲染成空列表」「旧响应覆盖新响应」「loading 被先完成的请求
 * 提前清掉」等缺陷。这里把四态收敛为可辨识联合：渲染处穷尽分支后，不可能
 * 再组合出矛盾状态。
 */
export type Async<T> =
  | { status: "idle" }
  | { status: "loading"; previous?: T }
  | { status: "ready"; data: T }
  | { status: "error"; message: string; previous?: T };

/** 从任意抛出的值读出人可读的错误文案（Tauri 命令失败通常是 string）。 */
export function errorMessage(cause: unknown, fallback = "操作未完成，请重试。"): string {
  if (typeof cause === "string") return cause.trim() || fallback;
  if (cause instanceof Error) return cause.message.trim() || fallback;
  if (typeof cause === "object" && cause !== null) {
    const message = (cause as { message?: unknown }).message;
    if (typeof message === "string" && message.trim()) return message;
  }
  return fallback;
}

/**
 * 防旧响应覆盖的请求 Hook。
 *
 * 同一数据源在筛选/参数变化时可能先后发出多个请求；只有最后一个允许写入
 * 状态，先完成者被丢弃。`loading` 只反映最新请求，不会被旧请求的 finally
 * 清掉。请求以 `run` 触发，`enabled` 切换时不自动重发。
 */
export function useLatestRequest<T>(fetch: (signal: { cancelled: boolean }) => Promise<T>) {
  const [state, setState] = useState<Async<T>>({ status: "idle" });
  const sequence = useRef(0);
  const fetchRef = useRef(fetch);
  fetchRef.current = fetch;

  const run = useCallback(async (previous?: T): Promise<boolean> => {
    const ticket = ++sequence.current;
    const signal = { cancelled: false };
    setState((old) => ({
      status: "loading",
      previous: old.status === "ready" ? old.data : old.status === "loading" ? old.previous : previous,
    }));
    try {
      const data = await fetchRef.current(signal);
      if (signal.cancelled || ticket !== sequence.current) return false;
      setState({ status: "ready", data });
      return true;
    } catch (cause) {
      if (signal.cancelled || ticket !== sequence.current) return false;
      setState((old) => ({
        status: "error",
        message: errorMessage(cause),
        previous: old.status === "loading" ? old.previous : previous,
      }));
      return false;
    }
  }, []);

  const reset = useCallback(() => {
    sequence.current++;
    setState({ status: "idle" });
  }, []);

  return { state, run, reset };
}
