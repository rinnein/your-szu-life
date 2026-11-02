import { useCallback, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useChannelSubscription } from "../tauri-channel";
import { formatDateKey } from "./calendar-math";
import type { ScheduleCategory, ScheduleItem } from "./types";
import { useElectricityMonitor } from "../electricity-monitor";
import type { OjDeadline } from "../oj/types";
import type { UoocSnapshot } from "../uooc/types";
import { useUoocAssessments } from "../uooc/use-assessments";
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

type ChannelEvent<T> = { type: "initial" | "snapshot"; data: T };

export function useHomeDeadlines() {
  const electricity = useElectricityMonitor();
  const [ojDeadlines, setOjDeadlines] = useState<OjDeadline[]>([]);
  const [uoocSnapshot, setUoocSnapshot] = useState<UoocSnapshot>();
  const [lmsSnapshot, setLmsSnapshot] = useState<LmsSnapshot>();
  const assessments = useUoocAssessments();
  const [ready, setReady] = useState({ oj: false, uooc: false, lms: false });
  const [refreshing, setRefreshing] = useState(false);

  // 首页待办全部由后端 Channel 推送，不再按定时器或事件重拉快照/列表。
  useChannelSubscription<ChannelEvent<OjDeadline[]>>(
    "subscribe_oj_deadlines",
    (event) => {
      if (!Array.isArray(event?.data)) return;
      setOjDeadlines(event.data);
      setReady((old) => (old.oj ? old : { ...old, oj: true }));
    },
    {},
    { onError: () => setReady((old) => (old.oj ? old : { ...old, oj: true })) },
  );

  useChannelSubscription<ChannelEvent<UoocSnapshot>>(
    "subscribe_uooc_state",
    (event) => {
      if (!event?.data) return;
      setUoocSnapshot(event.data);
      setReady((old) => (old.uooc ? old : { ...old, uooc: true }));
    },
    {},
    { onError: () => setReady((old) => (old.uooc ? old : { ...old, uooc: true })) },
  );

  useChannelSubscription<ChannelEvent<LmsSnapshot>>(
    "subscribe_lms_state",
    (event) => {
      if (!event?.data) return;
      setLmsSnapshot(event.data);
      setReady((old) => (old.lms ? old : { ...old, lms: true }));
    },
    {},
    { onError: () => setReady((old) => (old.lms ? old : { ...old, lms: true })) },
  );

  // 用户主动刷新时才触发一次后端刷新；日常更新由 Channel 推送。
  const refresh = useCallback(async () => {
    setRefreshing(true);
    await Promise.allSettled([invoke("refresh_oj"), invoke("refresh_lms"), assessments.refresh()]);
    setRefreshing(false);
  }, [assessments.refresh]);

  const ojItems = useMemo<ScheduleItem[]>(() => {
    const now = Date.now();
    const items: ScheduleItem[] = [];
    for (const contest of ojDeadlines) {
      if (contest.end <= now) continue;
      if (contest.hasProgress && contest.total > 0 && contest.accepted >= contest.total) continue;
      const targetDate = new Date(contest.end);
      items.push({
        id: `oj:${contest.siteId}:${contest.id}`,
        category: "oj-quiz",
        title: contest.title,
        subtitle: `${contest.siteName} · ${contest.isExam ? "考试" : "测验"}`,
        targetDate,
        dateKey: formatDateKey(targetDate),
        actionUrl: `/study/oj?site=${encodeURIComponent(contest.siteId)}&contest=${encodeURIComponent(contest.id)}`,
        urgent: contest.end - now < 86_400_000,
      });
    }
    return items;
  }, [ojDeadlines]);

  // UOOC 的课程结课、考试与作业都来自后端 Channel 推送，这里只做派生。
  const uoocItems = useMemo<ScheduleItem[]>(() => {
    if (!uoocSnapshot || uoocSnapshot.session !== "ready") return [];

    const items: ScheduleItem[] = [];
    const now = Date.now();

    // 1. UOOC 课程结课时间
    for (const course of uoocSnapshot.courses || []) {
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
    for (const exam of assessments.exams) {
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

    // 3. UOOC 作业截止时间
    for (const hw of assessments.homeworks) {
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

    return items;
  }, [uoocSnapshot, assessments.exams, assessments.homeworks]);

  const lmsItems = useMemo<ScheduleItem[]>(() => {
    if (!lmsSnapshot || lmsSnapshot.session !== "ready") return [];
    const now = Date.now();
    const items: ScheduleItem[] = [];
    for (const todo of lmsSnapshot.todos || []) {
      const endDate = new Date(todo.end_time);
      if (isNaN(endDate.getTime()) || endDate.getTime() <= now) continue;
      items.push(lmsItem(todo, endDate, now));
    }
    return items;
  }, [lmsSnapshot]);

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
    loading: refreshing || !ready.oj || !ready.uooc || !ready.lms,
    refresh,
  };
}
