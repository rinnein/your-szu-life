import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isChildDocument } from "../logical-navigation";
import { connectCph } from "./cph-transport";
import type { Snapshot } from "./types";
export function OjCompanionBridge() {
  const [snapshot, setSnapshot] = useState<Snapshot>();
  const owner = "__TAURI_INTERNALS__" in window && !isChildDocument();
  useEffect(() => {
    if (!owner) return;
    let live = true;
    let stop: (() => void) | undefined;
    void listen<Snapshot>("oj-state-changed", ({ payload }) => {
      if (live) setSnapshot(payload);
    })
      .then(async (off) => {
        if (!live) {
          off();
          return;
        }
        stop = off;
        const value = await invoke<Snapshot>("get_oj_snapshot");
        if (live) setSnapshot(value);
      })
      .catch(() => {});
    return () => {
      live = false;
      stop?.();
    };
  }, [owner]);
  const enabled =
    !!snapshot?.desktop &&
    snapshot.companionEnabled === true &&
    snapshot.sites.some((site) => site.enabled);
  const port = snapshot?.companionPort ?? 27121;
  useEffect(() => {
    if (!owner || !enabled) return;
    const bridge = connectCph(
      port,
      (status) => {
        void invoke("set_oj_companion_status", { port, status }).catch(() => {});
      },
      (input) => invoke("submit_oj_companion", { input }),
    );
    let live = true;
    let stop: (() => void) | undefined;
    void listen("oj-companion-activate", () => bridge.activate())
      .then((off) => {
        if (live) stop = off;
        else off();
      })
      .catch(() => {});
    return () => {
      live = false;
      stop?.();
      bridge.disconnect();
    };
  }, [owner, enabled, port]);
  return null;
}
