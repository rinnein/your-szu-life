import { useLogicalNavigation } from "./logical-navigation";
import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { PluginListener } from "@tauri-apps/api/core";

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

export function ElectricityMonitorProvider({ children }: { children: ReactNode }) {
  const navigation = useLogicalNavigation();
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

  useEffect(() => {
    const reportVisibility = () => {
      void invoke("set_application_visibility", {
        visible: document.visibilityState === "visible",
      }).catch(() => undefined);
    };
    reportVisibility();
    document.addEventListener("visibilitychange", reportVisibility);
    return () => document.removeEventListener("visibilitychange", reportVisibility);
  }, []);

  useEffect(() => {
    let active = true;
    let stop: PluginListener | undefined;
    void import("@tauri-apps/plugin-notification")
      .then(async ({ onAction }) => {
        const listener = await onAction((event) => {
          const extra = event.notification?.extra as { route?: string } | undefined;
          if (
            active &&
            (extra?.route === "/campus-payments/recharge" ||
              event.notification?.actionTypeId === "electricity-low" ||
              event.actionId === "open-recharge" ||
              event.actionId === "electricity-low")
          ) {
            void navigation.push("/campus-payments/recharge");
          }
        });
        if (active) stop = listener;
        else await listener.unregister();
      })
      .catch(() => undefined);
    return () => {
      active = false;
      void stop?.unregister();
    };
  }, [navigation]);

  return <Context.Provider value={status}>{children}</Context.Provider>;
}

export function useElectricityMonitor() {
  return useContext(Context) ?? EMPTY_ELECTRICITY_STATUS;
}
