import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useRouter } from "@tanstack/react-router";
import { useLogicalNavigation } from "../logical-navigation";
import { isChildDocument } from "../logical-navigation";
import { route, type Notice, type Snapshot } from "./types";
export function OjNotifications() {
  const nav = useLogicalNavigation();
  const router = useRouter();
  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window) || isChildDocument()) return;
    let active = true;
    const stops: (() => void)[] = [];
    const open = async (notice: Notice) => {
      if (
        !active ||
        !notice ||
        typeof notice.siteId !== "string" ||
        !(notice.contestId === "" || /^\d+$/.test(notice.contestId))
      )
        return;
      const snapshot = await invoke<Snapshot>("get_oj_snapshot");
      if (
        !snapshot.sites.some(
          (s) => s.id === notice.siteId && s.username === notice.account && s.enabled,
        )
      )
        return;
      const target = route({ site: notice.siteId, contest: notice.contestId });
      await (router.state.location.pathname === "/oj" ? nav.replace(target) : nav.push(target));
    };
    void listen<Notice>("oj-open", (e) => void open(e.payload)).then((stop) =>
      active ? stops.push(stop) : stop(),
    );
    void import("@tauri-apps/plugin-notification")
      .then(({ onAction }) =>
        onAction((event) => {
          const notice = event.notification?.extra?.ojNotice;
          if (notice) void open(notice as Notice);
        }),
      )
      .then((listener) =>
        active ? stops.push(() => void listener.unregister()) : void listener.unregister(),
      )
      .catch(() => {});
    const refresh = () => {
      if (document.visibilityState === "visible") void invoke("refresh_oj").catch(() => {});
    };
    document.addEventListener("visibilitychange", refresh);
    return () => {
      active = false;
      stops.forEach((s) => s());
      document.removeEventListener("visibilitychange", refresh);
    };
  }, [nav, router]);
  return null;
}
