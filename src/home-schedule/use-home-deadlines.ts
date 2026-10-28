import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { formatDateKey } from "./calendar-math";
import type { ScheduleCategory, ScheduleItem } from "./types";
import { useElectricityMonitor } from "../electricity-monitor";
import { ojProgressCache, progressScope } from "../oj/progress-store";
import type { Contest, Group, Snapshot } from "../oj/types";

export function useHomeDeadlines() {
  const electricity = useElectricityMonitor();
  const [ojItems, setOjItems] = useState<ScheduleItem[]>([]);
  const [loading, setLoading] = useState(true);

  const fetchOjDeadlines = useCallback(async () => {
    try {
      const snapshot = await invoke<Snapshot>("get_oj_snapshot");
      if (!snapshot?.sites) return;

      const items: ScheduleItem[] = [];
      const seen = new Set<string>();
      const now = Date.now();

      for (const site of snapshot.sites) {
        if (!site.enabled || site.session !== "ready") continue;

        let groups: Group[] = [];
        try {
          groups = await invoke<Group[]>("get_oj_groups", { siteId: site.id });
        } catch {
          continue;
        }

        for (const group of groups) {
          let contests: Contest[] = [];
          try {
            contests = await invoke<Contest[]>("get_oj_contests", {
              siteId: site.id,
              groupId: group.id,
            });
          } catch {
            continue;
          }

          for (const contest of contests) {
            const contestKey = `${site.id}:${contest.id}`;
            if (seen.has(contestKey)) continue;

            const contestNow = contest.now || now;
            // 截止时间必须在未来
            if (contestNow >= contest.end) continue;

            // 检查是否已经全部 AC
            let isCompleted = false;
            try {
              const scope = progressScope(site);
              const progress = await ojProgressCache.ensure(scope, contest.id);
              if (progress && progress.total > 0 && progress.accepted === progress.total) {
                isCompleted = true;
              }
            } catch {
              // Ignore progress error
            }

            if (!isCompleted) {
              seen.add(contestKey);
              const targetDate = new Date(contest.end);
              items.push({
                id: `oj:${site.id}:${contest.id}`,
                category: "oj-quiz",
                title: contest.title,
                subtitle: `${site.name} · ${contest.isExam ? "考试" : "测验"}`,
                targetDate,
                dateKey: formatDateKey(targetDate),
                actionUrl: `/study/oj?site=${encodeURIComponent(site.id)}&contest=${encodeURIComponent(contest.id)}`,
                urgent: contest.end - now < 86_400_000,
              });
            }
          }
        }
      }

      setOjItems(items);
    } catch {
      // Ignore network errors
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    let active = true;
    void fetchOjDeadlines();

    const unlistenState = listen("oj-state-changed", () => {
      if (active) void fetchOjDeadlines();
    });
    const unlistenProgress = listen("oj-progress-invalidated", () => {
      if (active) void fetchOjDeadlines();
    });

    const interval = setInterval(() => {
      if (active) void fetchOjDeadlines();
    }, 60_000);

    return () => {
      active = false;
      clearInterval(interval);
      void unlistenState.then((fn) => fn());
      void unlistenProgress.then((fn) => fn());
    };
  }, [fetchOjDeadlines]);

  // 计算电量预估断电事项
  const electricityItems = useMemo<ScheduleItem[]>(() => {
    if (
      electricity.expired ||
      electricity.estimatedDaysLeft == null ||
      electricity.estimatedDaysLeft < 0
    ) {
      return [];
    }

    const baseTime = electricity.observedDate
      ? new Date(electricity.observedDate + "T00:00:00").getTime()
      : Date.now();
    const outageTimestamp = baseTime + electricity.estimatedDaysLeft * 86_400_000;
    const targetDate = new Date(outageTimestamp);
    const remainingText =
      electricity.remaining != null ? `（剩余约 ${electricity.remaining.toFixed(1)} 度）` : "";

    return [
      {
        id: "electricity:outage-forecast",
        category: "electricity-outage",
        title: `预计电量耗尽${remainingText}`,
        subtitle: "宿舍电费监控 · 预估断电日期",
        targetDate,
        dateKey: formatDateKey(targetDate),
        actionUrl: "/life/campus-payments/recharge",
        urgent: electricity.estimatedDaysLeft < 1.0,
      },
    ];
  }, [electricity]);

  // 合并全部事项并排序
  const items = useMemo(() => {
    return [...ojItems, ...electricityItems].sort(
      (a, b) => a.targetDate.getTime() - b.targetDate.getTime(),
    );
  }, [ojItems, electricityItems]);

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
    loading,
    refresh: fetchOjDeadlines,
  };
}
