import { useCallback, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useChannelSubscription } from "../tauri-channel";
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

export function useHomeDeadlines() {
  const [todoItems, setTodoItems] = useState<TodoItem[]>([]);
  const [ready, setReady] = useState(false);
  const [refreshing, setRefreshing] = useState(false);

  // 首页待办全部由后端 `todos` 模块聚合后经 Channel 推送，前端不再合并多路数据。
  useChannelSubscription<TodosEvent>(
    "subscribe_todos",
    (event) => {
      if (!Array.isArray(event?.data)) return;
      setTodoItems(event.data);
      setReady(true);
    },
    {},
    { onError: () => setReady(true) },
  );

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
    refresh,
  };
}
