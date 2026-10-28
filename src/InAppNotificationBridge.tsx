import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { useLogicalNavigation } from "./logical-navigation";
import { toast } from "./ui";

export interface InAppNotificationPayload {
  category: "campus_network" | "electricity" | "enrollment" | "oj" | "uooc";
  title: string;
  body: string;
  route?: string | null;
  actionLabel?: string | null;
  extra?: unknown;
  id?: number | null;
  actionTypeId?: string | null;
}

export function InAppNotificationBridge() {
  const navigation = useLogicalNavigation();

  useEffect(() => {
    if (typeof window === "undefined") return;

    const syncFocus = () => {
      const isFocused =
        document.visibilityState === "visible" &&
        (typeof document.hasFocus === "function" ? document.hasFocus() : true);
      void invoke("set_application_focus", { focused: isFocused }).catch(() => {});
    };

    window.addEventListener("focus", syncFocus);
    window.addEventListener("blur", syncFocus);
    document.addEventListener("visibilitychange", syncFocus);
    syncFocus();

    let unlisten: (() => void) | undefined;
    if ("__TAURI_INTERNALS__" in window) {
      void listen<InAppNotificationPayload>("app://in-app-notification", (event) => {
        const payload = event.payload;
        if (!payload || !payload.title) return;

        const isWarning =
          payload.category === "electricity" ||
          payload.category === "campus_network" ||
          payload.title.includes("失败") ||
          payload.body.includes("不足");

        const notifyFn = isWarning ? toast.warning : toast.info;

        notifyFn(payload.title, {
          description: payload.body,
          actionLabel: payload.actionLabel ?? (payload.route ? "查看" : undefined),
          onAction: payload.route
            ? () => {
                void navigation.push(payload.route!);
              }
            : undefined,
        });
      }).then((stop) => {
        unlisten = stop;
      });
    }

    return () => {
      window.removeEventListener("focus", syncFocus);
      window.removeEventListener("blur", syncFocus);
      document.removeEventListener("visibilitychange", syncFocus);
      unlisten?.();
    };
  }, [navigation]);

  return null;
}
