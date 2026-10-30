import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { formatDateKey } from "./calendar-math";
import type { ScheduleCategory, ScheduleItem } from "./types";
import { useElectricityMonitor } from "../electricity-monitor";
import { ojProgressCache, progressScope } from "../oj/progress-store";
import type { Contest, Group, Snapshot as OjSnapshot } from "../oj/types";
import type { ExamTask, HomeworkTask, UoocSnapshot } from "../uooc/types";
import type { LmsSnapshot, LmsTodo } from "../lms/types";
import { lmsTodoUrl } from "../lms/link";

function parseDateSafely(raw: string | undefined): Date | null {
  if (!raw) return null;
  const normalized = raw.replace(/\//g, "-").trim();
  const full = normalized.includes(" ") ? normalized : `${normalized} 23:59:59`;
  const parsed = new Date(full);
  return isNaN(parsed.getTime()) ? null : parsed;
}

function lmsItem(todo: LmsTodo, endDate: Date, now: number): ScheduleItem {
  return {
    id: `lms:homework:${todo.id}`,
    category: "lms-homework",
    title: todo.title,
    subtitle: `畅课 · ${todo.course_name}`,
    targetDate: endDate,
    dateKey: formatDateKey(endDate),
    actionUrl: "/study/lms",
    externalUrl: lmsTodoUrl(todo),
    urgent: endDate.getTime() - now < 86_400_000,
  };
}

export function useHomeDeadlines() {
  const electricity = useElectricityMonitor();
  const [ojItems, setOjItems] = useState<ScheduleItem[]>([]);
  const [uoocItems, setUoocItems] = useState<ScheduleItem[]>([]);
  const [lmsItems, setLmsItems] = useState<ScheduleItem[]>([]);
  const [loading, setLoading] = useState(true);

  const fetchOjDeadlines = useCallback(async () => {
    try {
      const snapshot = await invoke<OjSnapshot>("get_oj_snapshot");
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
    }
  }, []);

  const fetchUoocDeadlines = useCallback(async () => {
    try {
      const snapshot = await invoke<UoocSnapshot>("get_uooc_snapshot");
      if (!snapshot || snapshot.session !== "ready") return;

      const items: ScheduleItem[] = [];
      const now = Date.now();

      // 1. UOOC 课程结课时间
      for (const course of snapshot.courses || []) {
        if (!course.endDate) continue;
        const endDate = parseDateSafely(course.endDate);
        if (endDate && endDate.getTime() > now) {
          items.push({
            id: `uooc:course:${course.id}`,
            category: "uooc-course-end",
            title: `${course.title} 课程结课`,
            subtitle: "UOOC · 课程结课",
            targetDate: endDate,
            dateKey: formatDateKey(endDate),
            actionUrl: "/study/uooc?tab=courses",
            urgent: endDate.getTime() - now < 86_400_000 * 3,
          });
        }
      }

      // 2. UOOC 考试截止时间
      try {
        const exams = await invoke<ExamTask[]>("list_uooc_exams", { cid: null });
        for (const exam of exams || []) {
          const isCompleted =
            exam.canView ||
            (exam.statusCode !== undefined && exam.statusCode >= 20) ||
            exam.statusText === "已批改" ||
            (exam.score !== null && exam.score !== undefined) ||
            (exam.chance > 0 && exam.submitCount >= exam.chance);

          if (isCompleted || !exam.endTime) continue;
          const endDate = parseDateSafely(exam.endTime);
          if (endDate && endDate.getTime() > now) {
            items.push({
              id: `uooc:exam:${exam.id}`,
              category: "uooc-exam",
              title: exam.title,
              subtitle: `${exam.courseName} · 考试`,
              targetDate: endDate,
              dateKey: formatDateKey(endDate),
              actionUrl: "/study/uooc?tab=exams&subtab=exam",
              urgent: endDate.getTime() - now < 86_400_000,
            });
          }
        }
      } catch {
        // Ignore exam fetch error
      }

      // 3. UOOC 作业截止时间
      try {
        const homeworks = await invoke<HomeworkTask[]>("list_uooc_homeworks", { cid: null });
        for (const hw of homeworks || []) {
          const isCompleted =
            hw.canView ||
            (hw.statusCode !== undefined && hw.statusCode >= 20) ||
            hw.statusText === "已批改" ||
            (hw.score !== null && hw.score !== undefined) ||
            (hw.chance > 0 && hw.submitCount >= hw.chance);

          if (isCompleted || !hw.endTime) continue;
          const endDate = parseDateSafely(hw.endTime);
          if (endDate && endDate.getTime() > now) {
            items.push({
              id: `uooc:homework:${hw.id}`,
              category: "uooc-homework",
              title: hw.title,
              subtitle: `${hw.courseName} · 作业`,
              targetDate: endDate,
              dateKey: formatDateKey(endDate),
              actionUrl: "/study/uooc?tab=exams&subtab=homework",
              urgent: endDate.getTime() - now < 86_400_000,
            });
          }
        }
      } catch {
        // Ignore homework fetch error
      }

      setUoocItems(items);
    } catch {
      // Ignore UOOC snapshot error
    }
  }, []);

  const fetchLmsDeadlines = useCallback(async () => {
    try {
      const snapshot = await invoke<LmsSnapshot>("get_lms_snapshot");
      if (!snapshot || snapshot.session !== "ready") return;

      const items: ScheduleItem[] = [];
      const now = Date.now();

      for (const todo of snapshot.todos || []) {
        const endDate = new Date(todo.end_time);
        if (isNaN(endDate.getTime()) || endDate.getTime() <= now) continue;
        items.push(lmsItem(todo, endDate, now));
      }

      setLmsItems(items);
    } catch {
      // Ignore LMS snapshot error
    }
  }, []);

  const refreshAll = useCallback(async () => {
    setLoading(true);
    await Promise.allSettled([fetchOjDeadlines(), fetchUoocDeadlines(), fetchLmsDeadlines()]);
    setLoading(false);
  }, [fetchOjDeadlines, fetchUoocDeadlines, fetchLmsDeadlines]);

  useEffect(() => {
    let active = true;
    void refreshAll();

    const unlistenState = listen("oj-state-changed", () => {
      if (active) void fetchOjDeadlines();
    });
    const unlistenProgress = listen("oj-progress-invalidated", () => {
      if (active) void fetchOjDeadlines();
    });
    const unlistenUooc = listen("uooc-state-changed", () => {
      if (active) void fetchUoocDeadlines();
    });
    const unlistenNotification = listen<{ category?: string }>(
      "app://in-app-notification",
      (event) => {
        if (!active) return;
        const cat = event.payload?.category;
        if (cat === "uooc") void fetchUoocDeadlines();
        else if (cat === "oj") void fetchOjDeadlines();
      },
    );

    const interval = setInterval(() => {
      if (active) void refreshAll();
    }, 60_000);

    return () => {
      active = false;
      clearInterval(interval);
      void unlistenState.then((fn) => fn());
      void unlistenProgress.then((fn) => fn());
      void unlistenUooc.then((fn) => fn());
      void unlistenNotification.then((fn) => fn());
    };
  }, [fetchOjDeadlines, fetchUoocDeadlines, refreshAll]);

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
    return [...ojItems, ...uoocItems, ...lmsItems, ...electricityItems].sort(
      (a, b) => a.targetDate.getTime() - b.targetDate.getTime(),
    );
  }, [ojItems, uoocItems, lmsItems, electricityItems]);

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
    refresh: refreshAll,
  };
}
