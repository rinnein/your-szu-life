export type Mode = "campus" | "webvpn";
export type Gear = "low" | "medium" | "high" | "ultra";
export type Course = {
  id: string;
  code: string;
  name: string;
  teacher: string;
  category: string;
  credits: number | null;
  schedule: string;
  capacity: number | null;
  remaining: number | null;
  campus: string;
  mainSelect: boolean | null;
  hasTest: boolean;
  testIds: string[];
};
export type Task = {
  id: string;
  course: Course;
  oldCourse: Course | null;
  threshold: number;
  group: string | null;
  priority: number;
  gear: Gear;
  failures: number;
  phase: string;
  message: string;
  enabled: boolean;
  pending: unknown;
  startedAt: number | null;
};
export type Academic = {
  summary: Record<string, unknown>[];
  modules: { module: Record<string, unknown>; courses: Record<string, unknown>[] }[];
  calculatedAt: string;
  updatedAt: number;
};
export type Snapshot = {
  mode: Mode;
  context: { student: string; term: string; batch: string; campus: string };
  loggedIn: boolean;
  authentication?: "unknown" | "authenticated" | "expired";
  admission?: "unchecked" | "ready" | "restricted" | "initializing" | "unavailable";
  admissionMessage?: string;
  message: string;
  courses: Course[];
  selected: Course[];
  tasks: Task[];
  pins: string[];
  updatedAt: number | null;
  complete: boolean;
  running: boolean;
  academic: Academic | null;
  query: { category: string; keyword: string; page: number };
};
export type Probe = { mode: Mode; latency: number | null; needs2fa: boolean; message: string };
export function defaultMode(probes: Probe[]): Mode {
  return (
    probes
      .filter((p) => p.latency !== null && !p.needs2fa)
      .sort(
        (a, b) =>
          (a.latency ?? Infinity) - (b.latency ?? Infinity) || (a.mode === "campus" ? -1 : 1),
      )[0]?.mode ?? "campus"
  );
}
export const gears: Record<Gear, string> = {
  low: "low · 60秒",
  medium: "medium · 10秒",
  high: "high · 1秒",
  ultra: "ultra · 0.25秒",
};
export function academicMatch(academic: Academic | null, code: string) {
  if (!code) return undefined;
  return academic?.modules
    .flatMap((m) => m.courses.map((c) => ({ ...c, group: m.module }) as Record<string, unknown>))
    .filter((c) => String(c.KCH) === code)
    .sort(
      (a, b) =>
        Number(String(b.SFTG) === "1") - Number(String(a.SFTG) === "1") ||
        Number(String(b.SFXK) === "1") - Number(String(a.SFXK) === "1"),
    )[0];
}
export function academicStatus(row: Record<string, unknown> | undefined) {
  if (!row) return "学业状态未知";
  if (String(row.SFTG) === "1") return "已完成";
  if (String(row.SFXK) === "1") return "已选未完成";
  if (String(row.SFTG) === "2") {
    if (String(row.KCXZDM_DISPLAY) === "必修") return "必修未完成";
    const group = row.group as Record<string, unknown> | undefined;
    if (group && (String(group.SFTG) === "1" || Number(group.WCXF) >= Number(group.YQXF)))
      return "未修 · 课组已满足";
    return "未修 · 课组候选";
  }
  return "学业状态未知";
}
export type Slot = {
  day: number;
  start: number;
  end: number;
  weeks: number[];
  course: Course;
  pending: boolean;
};
export function timetable(
  selected: Course[],
  tasks: Task[],
): { slots: Slot[]; unscheduled: Course[] } {
  const courses = [
    ...selected.map((course) => ({ course, pending: false })),
    ...tasks
      .filter(
        (t) => t.enabled && t.phase !== "completed" && !selected.some((c) => c.id === t.course.id),
      )
      .map((t) => ({ course: t.course, pending: true })),
  ];
  const slots: Slot[] = [];
  const unscheduled: Course[] = [];
  for (const { course, pending } of courses) {
    let found = false;
    const pattern =
      /((?:\d{1,2}(?:\s*[-–—~～至]\s*\d{1,2})?周(?:\s*[（(][单双][）)])?\s*[,，、]?\s*)+)?((?:星期|周)[一二三四五六日天](?:\s*[,，、/&和及]\s*(?:(?:星期|周))?[一二三四五六日天])*)\s*第?\s*(\d{1,2})(?:\s*[-–—~～至]\s*(\d{1,2}))?\s*节/g;
    for (const m of course.schedule.matchAll(pattern)) {
      const start = Number(m[3]),
        end = Number(m[4] ?? m[3]);
      if (start < 1 || end > 14 || end < start) continue;
      const weeks: number[] = [];
      if (m[1]) {
        for (const w of m[1].matchAll(
          /(\d{1,2})(?:\s*[-–—~～至]\s*(\d{1,2}))?周(?:\s*[（(]([单双])[）)])?/g,
        )) {
          const first = Number(w[1]),
            last = Number(w[2] ?? w[1]);
          for (let week = first; week <= Math.min(last, 30); week++)
            if (
              week >= 1 &&
              (!w[3] || week % 2 === (w[3] === "单" ? 1 : 0)) &&
              !weeks.includes(week)
            )
              weeks.push(week);
        }
      } else weeks.push(...Array.from({ length: 30 }, (_, i) => i + 1));
      for (const dayName of new Set(m[2].match(/[一二三四五六日天]/g) ?? []))
        slots.push({
          day: Math.min(7, "一二三四五六日天".indexOf(dayName) + 1),
          start,
          end,
          weeks,
          course,
          pending,
        });
      found = true;
    }
    if (!found) unscheduled.push(course);
  }
  return { slots, unscheduled };
}
export function conflicts(a: Slot, b: Slot) {
  return (
    a.course.id !== b.course.id &&
    a.day === b.day &&
    a.start <= b.end &&
    b.start <= a.end &&
    a.weeks.some((w) => b.weeks.includes(w))
  );
}

export function academicGroups(academic: Academic) {
  const result: (Academic["modules"][number] & { depth: number })[] = [];
  const seen = new Set<string>();
  const visit = (entry: Academic["modules"][number], depth: number) => {
    const id = String(entry.module.KZH);
    if (seen.has(id)) return;
    seen.add(id);
    result.push({ ...entry, depth });
    for (const child of academic.modules)
      if (String(child.module.FKZH) === id) visit(child, depth + 1);
  };
  for (const entry of academic.modules)
    if (!academic.modules.some((p) => String(p.module.KZH) === String(entry.module.FKZH)))
      visit(entry, 0);
  for (const entry of academic.modules) visit(entry, 0);
  return result;
}

export function nearSchoolRestart(now = Date.now()) {
  const minute = (((Math.floor(now / 60000) + 480) % 1440) + 1440) % 1440;
  return [16 * 60, 20 * 60].some((t) => minute >= t - 2 && minute <= t + 6);
}
