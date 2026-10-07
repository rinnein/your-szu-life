export const enrollmentPages = [
  { path: "/enrollment", label: "查找课程" },
  { path: "/enrollment/selected", label: "已选课程" },
  { path: "/enrollment/tasks", label: "抢课任务" },
  { path: "/enrollment/timetable", label: "选课课表" },
  { path: "/enrollment/settings", label: "选课设置" },
] as const;
export function legacyDestination(tab: unknown) {
  return {
    courses: "/enrollment",
    selected: "/enrollment/selected",
    tasks: "/enrollment/tasks",
    timetable: "/enrollment/timetable",
  }[String(tab)];
}
