import { useCallback, useRef, useState } from "react";

/**
 * 按键登记的进行中操作。
 *
 * 一个布尔 `busy` 管多个命令时，先完成的操作会把仍在进行的操作一起解锁。
 * 这里每个操作用独立的键（如 `task:<cid>`、`session`）：同键重复触发直接
 * 忽略，互不干扰，组件按 `isPending(key)` 渲染各自的进行态。
 */
export function usePending() {
  const [keys, setKeys] = useState<ReadonlySet<string>>(() => new Set());
  const live = useRef(new Set<string>());

  const isPending = useCallback(
    (...requested: string[]) => requested.some((key) => keys.has(key)),
    [keys],
  );

  /**
   * 运行一个以 `key` 登记的操作。同键已有操作在跑时返回 `{ ok: false, skipped: true }`，
   * 不再发起请求；成功返回 `{ ok: true, value }`，失败返回 `{ ok: false, error }`。
   * 失败的提示由调用方决定（通常 toast 一次），这里不做双重提示。
   */
  const run = useCallback(
    async <T,>(key: string, operation: () => Promise<T>): Promise<PendingResult<T>> => {
      if (live.current.has(key)) return { ok: false, skipped: true as const };
      live.current.add(key);
      setKeys((old) => new Set(old).add(key));
      try {
        const value = await operation();
        return { ok: true, value };
      } catch (cause) {
        return { ok: false, error: cause };
      } finally {
        live.current.delete(key);
        setKeys((old) => {
          const next = new Set(old);
          next.delete(key);
          return next;
        });
      }
    },
    [],
  );

  return { isPending, run, anyPending: keys.size > 0 };
}

export type PendingResult<T> =
  | { ok: true; value: T }
  | { ok: false; skipped?: true; error?: unknown };
