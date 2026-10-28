import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** 首页图标连击计数：2 秒内累计 5 次即启用调试模式。 */
export function registerDebugTap(taps: number[], now: number) {
  const recent = taps.filter((timestamp) => now - timestamp <= 2000);
  recent.push(now);
  return { activated: recent.length >= 5, taps: recent.length >= 5 ? [] : recent };
}

/**
 * 调试模式状态。
 *
 * 两个使用方：首页图标 5 连击（启用）与设置页的「诊断日志」tab（可见性）。
 * 两者不会同时挂载，因此各自订阅一次即可。
 */
export function useDebugMode() {
  const [enabled, setEnabled] = useState(false);
  const tapTimes = useRef<number[]>([]);

  useEffect(() => {
    let active = true;
    void invoke("get_debug_mode")
      .then((value) => {
        if (active) setEnabled(Boolean((value as { enabled?: boolean })?.enabled));
      })
      .catch(() => undefined);
    let unlisten: (() => void) | undefined;
    void listen<{ enabled: boolean }>("debug-mode-changed", (event) => {
      if (active) setEnabled(event.payload.enabled);
    }).then((stop) => {
      if (active) unlisten = stop;
      else stop();
    });
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  const enable = useCallback(async () => {
    await invoke("set_debug_mode", { enabled: true });
    setEnabled(true);
  }, []);

  const handleTap = useCallback(() => {
    const result = registerDebugTap(tapTimes.current, Date.now());
    if (result.activated) {
      tapTimes.current = [];
      void enable().catch(() => undefined);
      return;
    }
    tapTimes.current = result.taps;
  }, [enable]);

  return { enabled, enable, handleTap };
}
