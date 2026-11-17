import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useChannelSubscription } from "../tauri-channel";
import { createAccountCache } from "../account-cache";
import { formatDateKey } from "./calendar-math";
import type { ScheduleCategory, ScheduleItem } from "./types";

/**
 * 后端 `todos` 模块聚合后的待办项，字段与 `todos::model::TodoItem` 对应。
 * `kind` 直接复用首页日历的 `ScheduleCategory` 字面量。
 */
interface TodoItem {
  id: string;
  kind: ScheduleCategory;
  title: string;
  subtitle?: string | null;
  dueAtMs: number;
  route: string;
  externalUrl?: string | null;
  urgent: boolean;
}

type TodosEvent = { type: "initial" | "snapshot"; data: TodoItem[] };

// 待办列表按账号缓存：离开首页再回来时首帧就有内容，排序不会先落到默认顺序。
const itemsCache = createAccountCache<TodoItem[]>();

export function useHomeDeadlines(account?: string) {
  const [todoItems, setTodoItems] = useState<TodoItem[]>(() => itemsCache.read(account) ?? []);
  const [ready, setReady] = useState(() => itemsCache.read(account) !== undefined);
  const [refreshing, setRefreshing] = useState(false);
  const accountRef = useRef(account);

  // 首页待办全部由后端 `todos` 模块聚合后经 Channel 推送，前端不再合并多路数据。
  useChannelSubscription<TodosEvent>(
    "subscribe_todos",
    (event) => {
      if (!Array.isArray(event?.data)) return;
      itemsCache.write(account, event.data);
      setTodoItems(event.data);
      setReady(true);
    },
    {},
    { key: account, onError: () => setReady(true) },
  );

  // 账号切换（含退出登录）时丢弃上一个账号的待办；首次认证后则补上缓存。
  useEffect(() => {
    if (accountRef.current === account) return;
    accountRef.current = account;
    const cached = itemsCache.read(account);
    setTodoItems(cached ?? []);
    setReady(cached !== undefined);
  }, [account]);

  // 用户主动刷新时才触发一次后端刷新；日常更新由 Channel 推送。
  const refresh = useCallback(async () => {
    setRefreshing(true);
    try {
      await invoke("refresh_todos");
    } catch {
      // 刷新失败时保留当前列表，等下一次 Channel 推送或自动刷新。
    } finally {
      setRefreshing(false);
    }
  }, []);

  const items = useMemo<ScheduleItem[]>(() => {
    return todoItems
      .map((item) => {
        const targetDate = new Date(item.dueAtMs);
        return {
          id: item.id,
          category: item.kind,
          title: item.title,
          subtitle: item.subtitle ?? undefined,
          targetDate,
          dateKey: formatDateKey(targetDate),
          actionUrl: item.route,
          externalUrl: item.externalUrl ?? undefined,
          urgent: item.urgent,
        };
      })
      .sort((a, b) => a.targetDate.getTime() - b.targetDate.getTime());
  }, [todoItems]);

  // 构建日期到类别的快速索引 Map
  const categoryMap = useMemo(() => {
    const map = new Map<string, ScheduleCategory[]>();
    for (const item of items) {
      const list = map.get(item.dateKey) ?? [];
      if (!list.includes(item.category)) {
        list.push(item.category);
      }
      map.set(item.dateKey, list);
    }
    return map;
  }, [items]);

  return {
    items,
    categoryMap,
    loading: refreshing || !ready,
    /** 首次 Channel 推送（或订阅失败）后为 true，供卡片排序判断数据是否就绪。 */
    ready,
    refresh,
  };
}
