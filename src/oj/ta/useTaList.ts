import { useCallback, useEffect, useRef, useState } from "react";
import { errorMessage, type Async } from "../../async-state";
import { readTaCache, writeTaCache, type TaCacheValue } from "./ta-cache";

/**
 * 进入列表时读取前端 TTL 缓存；未过期直接渲染，不 invoke。
 * 没有任何定时器或后台刷新；手动 refresh 才会强制请求。
 */
export function useTaList<T>({
  cacheKey,
  load,
  enabled = true,
}: {
  cacheKey: string;
  load: () => Promise<T>;
  enabled?: boolean;
}) {
  const [state, setState] = useState<Async<TaCacheValue<T>>>(() => {
    const hit = readTaCache<T>(cacheKey);
    return hit && hit.fresh
      ? { status: "ready", data: { data: hit.data, updatedAt: hit.updatedAt } }
      : { status: "idle" };
  });
  const ticket = useRef(0);
  const loadRef = useRef(load);
  loadRef.current = load;

  const run = useCallback(
    async (force: boolean) => {
      const hit = readTaCache<T>(cacheKey);
      if (!force && hit?.fresh) {
        setState({
          status: "ready",
          data: { data: hit.data, updatedAt: hit.updatedAt },
        });
        return;
      }
      const current = ++ticket.current;
      setState((old) => ({
        status: "loading",
        previous: hit
          ? { data: hit.data, updatedAt: hit.updatedAt }
          : old.status === "ready"
            ? old.data
            : undefined,
      }));
      try {
        const data = await loadRef.current();
        if (current !== ticket.current) return;
        const updatedAt = Date.now();
        writeTaCache(cacheKey, data, updatedAt);
        setState({ status: "ready", data: { data, updatedAt } });
      } catch (cause) {
        if (current !== ticket.current) return;
        setState((old) => ({
          status: "error",
          message: errorMessage(cause),
          previous: old.status === "loading" ? old.previous : undefined,
        }));
      }
    },
    [cacheKey],
  );

  useEffect(() => {
    if (!enabled) {
      setState({ status: "idle" });
      return;
    }
    void run(false);
    return () => {
      // 卸载或 key 变化后忽略在途响应；不保留任何定时器。
      ticket.current++;
    };
  }, [cacheKey, enabled, run]);

  const refresh = useCallback(() => run(true), [run]);
  return { state, refresh };
}
