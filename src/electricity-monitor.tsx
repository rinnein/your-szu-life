import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type ElectricityStatus = {
  remaining: number | null;
  observedDate: string | null;
  expiresAtMs: number | null;
  expired: boolean;
  low: boolean;
  estimatedDaysLeft?: number | null;
  warningReason?: string | null;
};

export const EMPTY_ELECTRICITY_STATUS: ElectricityStatus = {
  remaining: null,
  observedDate: null,
  expiresAtMs: null,
  expired: false,
  low: false,
};

const Context = createContext<ElectricityStatus>(EMPTY_ELECTRICITY_STATUS);

function isStatus(value: unknown): value is ElectricityStatus {
  return Boolean(value && typeof value === "object" && "low" in value && "expired" in value);
}

/**
 * 电费状态订阅。
 *
 * 只有首页日历待办需要这份数据，因此挂在 `HomeCalendarWidget` 外层而不是应用根节点；
 * 前台/可见性上报与通知点击跳转已移到 `AppLifecycleBridge` / `NotificationCenter`。
 */
export function ElectricityMonitorProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<ElectricityStatus>(EMPTY_ELECTRICITY_STATUS);

  useEffect(() => {
    let active = true;
    let stop: (() => void) | undefined;
    void listen<ElectricityStatus>("electricity-status-changed", (event) => {
      if (active && isStatus(event.payload)) setStatus(event.payload);
    }).then((dispose) => {
      if (active) stop = dispose;
      else dispose();
    });
    void invoke<ElectricityStatus>("get_electricity_status")
      .then((next) => {
        if (active && isStatus(next)) setStatus(next);
      })
      .catch(() => undefined);
    return () => {
      active = false;
      stop?.();
    };
  }, []);

  return <Context.Provider value={status}>{children}</Context.Provider>;
}

export function useElectricityMonitor() {
  return useContext(Context) ?? EMPTY_ELECTRICITY_STATUS;
}
