import type { SemesterCourse, SessionCheckin, TimetableSession } from "./types";

export function localDateKey(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

export function todayKey(): string {
  return localDateKey(new Date());
}

export function dateFromKey(key: string | undefined): Date | undefined {
  if (!key || !/^\d{4}-\d{2}-\d{2}$/.test(key)) return undefined;
  const [year, month, day] = key.split("-").map(Number);
  const date = new Date(year, month - 1, day);
  return Number.isNaN(date.getTime()) ? undefined : date;
}

export function startOfDay(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

export function addDays(date: Date, days: number): Date {
  const next = new Date(date);
  next.setDate(next.getDate() + days);
  return next;
}

/** 周一作为一周起点。 */
export function startOfWeek(date: Date): Date {
  const day = startOfDay(date);
  const offset = (day.getDay() + 6) % 7;
  return addDays(day, -offset);
}

export function datesOfWeek(weekStart: Date): Date[] {
  return Array.from({ length: 7 }, (_, index) => addDays(weekStart, index));
}

export function weekdayLabel(date: Date): string {
  return `周${"日一二三四五六"[date.getDay()]}`;
}

export function shortDateLabel(date: Date): string {
  return `${date.getMonth() + 1}/${date.getDate()}`;
}

export function formatClock(ms: number): string {
  const date = new Date(ms);
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

export function normalizeCourseName(value: string | null | undefined): string {
  return (value ?? "").replace(/[\s·・,，.。()（）[\]【】《》<>_\-—–]/g, "").toLowerCase();
}

export function teacherFor(
  session: TimetableSession,
  courses: SemesterCourse[] | null | undefined,
): string | undefined {
  const name = normalizeCourseName(session?.courseName);
  if (!name) return undefined;
  const course = (courses ?? []).find((item) => normalizeCourseName(item?.name) === name);
  return course?.teacher?.trim() || undefined;
}

export function semesterPeriodLabel(course: SemesterCourse): string {
  return course.startPeriod === course.endPeriod
    ? `第${course.startPeriod}节`
    : `第${course.startPeriod}–${course.endPeriod}节`;
}

/** 把 `[1,2,3,5,7,8]` 压缩成 `1-3、5、7-8 周`。 */
export function formatWeeks(weeks: number[]): string {
  if (weeks.length === 0) return "";
  const sorted = [...new Set(weeks)].sort((a, b) => a - b);
  const ranges: string[] = [];
  let start = sorted[0];
  let previous = sorted[0];
  const flush = () => {
    ranges.push(start === previous ? `${start}` : `${start}-${previous}`);
  };
  for (const week of sorted.slice(1)) {
    if (week === previous + 1) {
      previous = week;
      continue;
    }
    flush();
    start = week;
    previous = week;
  }
  flush();
  return `${ranges.join("、")} 周`;
}

export function checkinMeta(checkin: SessionCheckin):
  | {
      text: string;
      tone: "warning" | "success";
    }
  | undefined {
  switch (checkin) {
    case "pending_lms":
      return { text: "待签到(畅课lms)", tone: "warning" };
    case "pending_szu":
      return { text: "待签到(SZU(实验性))", tone: "warning" };
    case "signed_lms":
    case "signed_szu":
      return { text: "已签到", tone: "success" };
    default:
      return undefined;
  }
}

export function isPendingCheckin(checkin: SessionCheckin): boolean {
  return checkin === "pending_lms" || checkin === "pending_szu";
}

export function isSignedCheckin(checkin: SessionCheckin): boolean {
  return checkin === "signed_lms" || checkin === "signed_szu";
}

/** 签到页深链；`open` 只在待签到时为 true。 */
export function signinRoute(
  session: TimetableSession,
  checkin: SessionCheckin,
  open: boolean,
): string | undefined {
  const course = encodeURIComponent(session.courseName);
  if (checkin === "pending_lms" || checkin === "signed_lms") {
    return `/study/signin?tab=lms&course=${course}${open ? "&open=1" : ""}`;
  }
  if (checkin === "pending_szu" || checkin === "signed_szu") {
    return `/study/signin?tab=szu&course=${course}&start=${session.startMs}${open ? "&open=1" : ""}`;
  }
  return undefined;
}

export type SessionStatus = "finished" | "current" | "next" | "future";

export function sessionStatus(
  session: TimetableSession,
  now: number,
  nextKey: string | undefined,
): SessionStatus {
  if (session.endMs <= now) return "finished";
  if (session.startMs <= now && now < session.endMs) return "current";
  if (`${session.courseName}|${session.startMs}` === nextKey) return "next";
  return "future";
}

export function nextSessionKey(sessions: TimetableSession[], now: number): string | undefined {
  const next = sessions
    .filter((session) => session.startMs > now)
    .sort((a, b) => a.startMs - b.startMs)[0];
  return next ? `${next.courseName}|${next.startMs}` : undefined;
}

export function sessionLocation(session: TimetableSession): string {
  const building = session.buildingName?.trim() || session.buildingNameEn?.trim() || "";
  const classroom = session.classroom?.trim() || "";
  if (building && classroom) return `${building} ${classroom}`;
  return building || classroom;
}

export type GridPlacement<T> = {
  dayIndex: number;
  startRow: number;
  span: number;
  entries: T[];
};

/**
 * 把课次按「星期 + 作息行」放进网格。
 *
 * 同一天、同一时段开始的课次合并进一个格子（上下堆叠），时间范围重叠的课次
 * 也并入先出现的格子并把行跨度撑到覆盖两者，避免两门课在同一位置互相遮挡。
 */
export function placeInGrid<T extends { dayIndex: number; startRow: number; endRow: number }>(
  entries: T[],
): GridPlacement<T>[] {
  const placements: GridPlacement<T>[] = [];
  const sorted = [...entries].sort((a, b) => a.dayIndex - b.dayIndex || a.startRow - b.startRow);
  for (const entry of sorted) {
    const start = Math.max(0, entry.startRow);
    const end = Math.max(start, entry.endRow);
    const host = placements.find(
      (placement) =>
        placement.dayIndex === entry.dayIndex &&
        placement.startRow <= end &&
        start <= placement.startRow + placement.span - 1,
    );
    if (host) {
      host.entries.push(entry);
      host.span = Math.max(host.span, end - host.startRow + 1);
      continue;
    }
    placements.push({
      dayIndex: entry.dayIndex,
      startRow: start,
      span: end - start + 1,
      entries: [entry],
    });
  }
  return placements;
}
