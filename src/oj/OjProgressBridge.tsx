import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { ojProgressCache } from "./progress-store";
import type { Snapshot } from "./types";

/** Each WebView reflects native invalidations; the native cache is shared across windows. */
export function OjProgressBridge() {
  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    let active = true;
    const disposers: (() => void)[] = [];
    const subscribe = async <T,>(event: string, callback: (value: T) => void) => {
      try {
        const off = await listen<T>(event, ({ payload }) => {
          if (active) callback(payload);
        });
        if (active) disposers.push(off);
        else off();
      } catch {
        /* Cache expiry still permits recovery if event subscription fails. */
      }
    };
    void subscribe("oj-progress-invalidated", ojProgressCache.invalidateNative);
    void subscribe<Snapshot>("oj-state-changed", (snapshot) =>
      ojProgressCache.reconcile(snapshot.sites),
    );
    return () => {
      active = false;
      disposers.forEach((off) => off());
    };
  }, []);
  return null;
}
