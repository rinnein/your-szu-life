import type { TimetableSnapshot } from "../../timetable/types";

function at(dayOffset: number, hour: number, minute: number): number {
  const date = new Date();
  date.setDate(date.getDate() + dayOffset);
  date.setHours(hour, minute, 0, 0);
  return date.getTime();
}

function dateKey(offset: number): string {
  const date = new Date();
  date.setDate(date.getDate() + offset);
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

const now = Date.now();
const weeks = Array.from({ length: 16 }, (_, index) => index + 1);

export const mockTimetableSnapshot: TimetableSnapshot = {
  fetchedAtMs: now,
  semester: "2026 秋",
  message: "",
  sessions: [
    {
      date: dateKey(0),
      courseName: "编译原理",
      buildingName: "致理楼",
      classroom: "301",
      buildingNameEn: null,
      startMs: at(0, 8, 30),
      endMs: at(0, 10, 5),
      semester: "2026 秋",
      checkin: "pending_lms",
    },
    {
      date: dateKey(0),
      courseName: "概率论与数理统计",
      buildingName: "汇星楼",
      classroom: "202",
      buildingNameEn: null,
      startMs: at(0, 14, 15),
      endMs: at(0, 15, 40),
      semester: "2026 秋",
      checkin: "none",
    },
    {
      date: dateKey(1),
      courseName: "计算机网络",
      buildingName: "致腾楼",
      classroom: "105",
      buildingNameEn: null,
      startMs: at(1, 10, 15),
      endMs: at(1, 11, 40),
      semester: "2026 秋",
      checkin: "none",
    },
  ],
  checkins: [
    {
      id: "checkin:demo:1",
      title: "编译原理 签到",
      subtitle: null,
      firstSeenMs: now - 2 * 60_000,
      route: "/study/signin?tab=lms",
    },
  ],
  semesterCourses: [
    {
      name: "编译原理",
      teacher: "演示教师甲",
      classroom: "A 栋 101",
      weekday: 1,
      startPeriod: 1,
      endPeriod: 2,
      weeks,
      rawWeeks: "1-16周",
    },
    {
      name: "计算机网络",
      teacher: "演示教师乙",
      classroom: "B 栋 202",
      weekday: 3,
      startPeriod: 3,
      endPeriod: 4,
      weeks,
      rawWeeks: "1-16周",
    },
    {
      name: "概率论与数理统计",
      teacher: "演示教师丙",
      classroom: "C 栋 303",
      weekday: 5,
      startPeriod: 6,
      endPeriod: 7,
      weeks,
      rawWeeks: "1-16周",
    },
  ],
  semesterFetchedAtMs: now,
  semesterMessage: "",
};
