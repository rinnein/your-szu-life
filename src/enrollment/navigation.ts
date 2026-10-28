export const enrollmentPages = [
  { path: "/study/enrollment", label: "查找课程" },
  { path: "/study/enrollment/selected", label: "已选课程" },
  { path: "/study/enrollment/tasks", label: "抢课任务" },
  { path: "/study/enrollment/timetable", label: "选课课表" },
  { path: "/study/enrollment/settings", label: "选课设置" },
] as const;
export function legacyDestination(tab: unknown) {
  return {
    courses: "/study/enrollment",
    selected: "/study/enrollment/selected",
    tasks: "/study/enrollment/tasks",
    timetable: "/study/enrollment/timetable",
  }[String(tab)];
}
