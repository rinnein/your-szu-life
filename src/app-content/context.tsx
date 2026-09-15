import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AppContentSnapshot } from "./types";

type ContentContext = {
  snapshot?: AppContentSnapshot;
  error?: string;
  refreshing: boolean;
  refresh: () => Promise<void>;
};

const Context = createContext<ContentContext | null>(null);

export function AppContentProvider({ children }: { children: ReactNode }) {
  const [snapshot, setSnapshot] = useState<AppContentSnapshot>();
  const [error, setError] = useState<string>();
  const [manualBusy, setManualBusy] = useState(false);
  const pending = useRef<Promise<void> | undefined>(undefined);
  const mounted = useRef(false);

  const accept = useCallback((next: AppContentSnapshot) => {
    if (!mounted.current) return;
    if (!next || typeof next.revision !== "number") {
      setError("无法读取公共内容服务，请在原生应用中重试。");
      return;
    }
    // Subscription and initial read may complete out of order.
    setSnapshot((current) => (current && current.revision > next.revision ? current : next));
    setError(undefined);
  }, []);

  useEffect(() => {
    mounted.current = true;
    let active = true;
    let unlisten: (() => void) | undefined;
    void (async () => {
      try {
        const stop = await listen<AppContentSnapshot>("app-content-changed", (event) => {
          if (active) accept(event.payload);
        });
        if (active) unlisten = stop;
        else stop();
      } catch {
        if (active) setError("无法订阅公共内容变化，请重新打开应用。");
      }
      if (!active) return;
      try {
        // Never refresh here: Rust alone owns startup / 24-hour scheduling.
        const next = await invoke<AppContentSnapshot>("get_app_content_snapshot");
        if (active) accept(next);
      } catch {
        if (active) setError("无法读取公共内容服务，请在原生应用中重试。");
      }
    })();
    return () => {
      active = false;
      mounted.current = false;
      unlisten?.();
    };
  }, [accept]);

  const refresh = useCallback(() => {
    if (pending.current) return pending.current;
    setManualBusy(true);
    setError(undefined);
    const request = invoke<AppContentSnapshot>("refresh_app_content")
      .then(accept)
      .catch(() => {
        if (mounted.current) setError("本次检查未能完成，请稍后重试。");
      })
      .finally(() => {
        pending.current = undefined;
        if (mounted.current) setManualBusy(false);
      });
    pending.current = request;
    return request;
  }, [accept]);

  return (
    <Context.Provider
      value={{ snapshot, error, refreshing: manualBusy || !!snapshot?.refreshing, refresh }}
    >
      {children}
    </Context.Provider>
  );
}

export function useAppContent() {
  const context = useContext(Context);
  if (!context) throw new Error("AppContentProvider is required");
  return context;
}
