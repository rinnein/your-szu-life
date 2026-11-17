import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useChannelSubscription } from "../tauri-channel";
import { createAccountCache } from "../account-cache";
import type { TimetableEvent, TimetableSnapshot } from "./types";

// 课表快照在进程内按账号缓存：首页与课表页切换、以及离开首页再回来时，
// 首帧就能拿到上一次的快照，不再闪一下「今天没有课」。
const snapshotCache = createAccountCache<TimetableSnapshot>();

/** 订阅课表快照；刷新失败只影响提示，不清空已有数据。 */
export function useTimetable(account?: string) {
  const [snapshot, setSnapshot] = useState<TimetableSnapshot | undefined>(() =>
    snapshotCache.read(account),
  );
  const [error, setError] = useState<string>();
  const [refreshing, setRefreshing] = useState(false);
  const accountRef = useRef(account);

  useChannelSubscription<TimetableEvent>(
    "subscribe_timetable",
    (event) => {
      if (!event?.data) return;
      snapshotCache.write(account, event.data);
      setSnapshot(event.data);
      setError(undefined);
    },
    {},
    {
      key: account,
      onError: (cause) => setError(typeof cause === "string" ? cause : "课表订阅失败，请稍后重试"),
    },
  );

  // 账号切换（含退出登录）时丢弃上一个账号的快照；首次认证后则补上缓存。
  useEffect(() => {
    if (accountRef.current === account) return;
    accountRef.current = account;
    setSnapshot(snapshotCache.read(account));
    setError(undefined);
  }, [account]);

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
