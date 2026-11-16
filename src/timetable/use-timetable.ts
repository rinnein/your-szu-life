import { useCallback, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useChannelSubscription } from "../tauri-channel";
import type { TimetableEvent, TimetableSnapshot } from "./types";

/** 订阅课表快照；刷新失败只影响提示，不清空已有数据。 */
export function useTimetable() {
  const [snapshot, setSnapshot] = useState<TimetableSnapshot>();
  const [error, setError] = useState<string>();
  const [refreshing, setRefreshing] = useState(false);

  useChannelSubscription<TimetableEvent>(
    "subscribe_timetable",
    (event) => {
      if (!event?.data) return;
      setSnapshot(event.data);
      setError(undefined);
    },
    {},
    {
      onError: (cause) => setError(typeof cause === "string" ? cause : "课表订阅失败，请稍后重试"),
    },
  );

  const refresh = useCallback(async (options?: { forceSemester?: boolean }) => {
    setRefreshing(true);
    try {
      await invoke("refresh_timetable", {
        forceSemester: options?.forceSemester ?? false,
      });
      setError(undefined);
    } catch (cause) {
      setError(typeof cause === "string" ? cause : "刷新课表失败");
    } finally {
      setRefreshing(false);
    }
  }, []);

  return { snapshot, error, refresh, refreshing };
}
