import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { createTaskChannel } from "../tauri-channel";
import { listen } from "@tauri-apps/api/event";
import type { Snapshot } from "./model";
let snapshot: Snapshot | undefined;
let sequence = 0;
let lastApplied = 0;
let accountEpoch = 0;
let expectedStudent: string | null | undefined;
let identityPending = false;
const accepts = (value: Snapshot) =>
  !identityPending &&
  expectedStudent !== null &&
  (expectedStudent === undefined || value.context.student === expectedStudent);
const subscribers = new Set<() => void>();
const subscribe = (fn: () => void) => {
  subscribers.add(fn);
  return () => {
    subscribers.delete(fn);
  };
};
const apply = (next: Snapshot) => {
  snapshot = next;

  subscribers.forEach((fn) => fn());
};
function isSnapshot(value: unknown): value is Snapshot {
  return !!value && typeof value === "object" && "context" in value && "tasks" in value;
}
let connections = 0;
let stops: Promise<() => void>[] = [];
export function useEnrollment(cacheOnly = false) {
  const state = useSyncExternalStore(subscribe, () => snapshot);
  const channels = useRef<Record<string, number>>({});
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [pending, setPending] = useState<Record<string, number>>({});
  const clearError = useCallback((key: string) => setErrors((old) => ({ ...old, [key]: "" })), []);
  const run = useCallback(
    async <T = Snapshot>(
      action: Record<string, unknown>,
      channel = String(action.type),
    ): Promise<T | undefined> => {
      if (cacheOnly) {
        if (action.type === "snapshot" || action.type === "selected")
          action = { type: "cacheSnapshot" };
        else if (action.type === "refresh") action = { type: "cache", query: action.query };
        else if (
          !["cacheSnapshot", "cache"].includes(String(action.type)) &&
          !(action.type === "control" && ["stop", "pause"].includes(String(action.action)))
        ) {
          setErrors((p) => ({ ...p, [channel]: "当前为只读缓存模式，请退出缓存模式后操作" }));
          return undefined;
        }
      }
      setPending((p) => ({ ...p, [channel]: (p[channel] ?? 0) + 1 }));
      clearError(channel);
      const started = ++sequence;
      const epoch = accountEpoch;
      channels.current[channel] = started;
      try {
        const value = await invoke<T>("enrollment_command", { action });
        if (
          isSnapshot(value) &&
          started >= lastApplied &&
          epoch === accountEpoch &&
          accepts(value)
        ) {
          lastApplied = started;
          apply(value);
        } else if (
          value &&
          typeof value === "object" &&
          "snapshot" in value &&
          isSnapshot(value.snapshot) &&
          started >= lastApplied &&
          epoch === accountEpoch &&
          accepts(value.snapshot)
        ) {
          lastApplied = started;
          apply(value.snapshot);
        }
        return value;
      } catch (e) {
        if (channels.current[channel] === started)
          setErrors((p) => ({ ...p, [channel]: typeof e === "string" ? e : "操作未完成，请重试" }));
        return undefined;
      } finally {
        setPending((p) => ({ ...p, [channel]: Math.max(0, (p[channel] ?? 1) - 1) }));
      }
    },
    [clearError, cacheOnly],
  );
  useEffect(() => {
    if (connections++ === 0) {
      const channel = createTaskChannel<{ type: string; data: Snapshot }>((e) => {
        if (isSnapshot(e.data) && accepts(e.data)) {
          lastApplied = ++sequence;
          apply(e.data);
        }
      });
      void invoke("subscribe_enrollment_state", { channel }).catch(() => {});

      stops = [
        listen<Snapshot>("enrollment-update", (e) => {
          if (isSnapshot(e.payload) && accepts(e.payload)) {
            lastApplied = ++sequence;
            apply(e.payload);
          }
        }),
        listen("auth-state-changed", async () => {
          const epoch = ++accountEpoch;
          identityPending = true;
          snapshot = undefined;
          lastApplied = ++sequence;
          subscribers.forEach((fn) => fn());
          try {
            const user = await invoke<{ studentId?: string } | null>("get_user_data");
            if (epoch !== accountEpoch) return;
            expectedStudent = user?.studentId ?? null;
          } catch {
            if (epoch !== accountEpoch) return;
            expectedStudent = null;
          }
          identityPending = false;
          void run({ type: "snapshot" });
        }),
      ];
      void run({ type: "snapshot" });
    }
    return () => {
      if (--connections === 0) {
        stops.forEach((p) => void p.then((stop) => stop()).catch(() => {}));
        stops = [];
      }
    };
  }, [run]);
  const official = useCallback(async () => {
    const key = "official";
    clearError(key);
    setPending((p) => ({ ...p, [key]: 1 }));
    try {
      await invoke("open_enrollment_official");
      return true;
    } catch (e) {
      setErrors((p) => ({ ...p, [key]: String(e) }));
      return false;
    } finally {
      setPending((p) => ({ ...p, [key]: 0 }));
    }
  }, [clearError]);
  const portal = useCallback(
    async (kind: "webvpn" | "academic") => {
      const key = `portal-${kind}`;
      clearError(key);
      setPending((p) => ({ ...p, [key]: 1 }));
      try {
        await invoke("open_academic_portal", { kind, target: null });
        return true;
      } catch (e) {
        setErrors((p) => ({ ...p, [key]: String(e) }));
        return false;
      } finally {
        setPending((p) => ({ ...p, [key]: 0 }));
      }
    },
    [clearError],
  );
  return {
    snapshot: state,
    errors,
    pending,
    clearError,
    run,
    portal,
    official,
    busy: Object.values(pending).some(Boolean),
    error: Object.values(errors).find(Boolean) ?? "",
  };
}
