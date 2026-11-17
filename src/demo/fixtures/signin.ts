/** 演示模式的签到/打卡数据（真实列表由 Rust 命令返回，这里给可交互的代表子集）。 */
import {
  localDateKey,
  type RollcallItem,
  type SigninAutomationSettings,
  type TimetableCache,
} from "../../signin/types";

export const mockSigninBuildings = [
  {
    id: 79,
    name: "致理楼",
    name_en: "Zhili Building",
    campus: "沧海校区",
    latitude_gcj02: 22.528,
    longitude_gcj02: 113.94,
    radius_m: 130,
    status: "listed",
  },
  {
    id: 73,
    name: "致腾楼",
    name_en: "Zhiteng Building",
    campus: "沧海校区",
    latitude_gcj02: 22.526,
    longitude_gcj02: 113.937,
    radius_m: 75,
    status: "listed",
  },
  {
    id: 84,
    name: "汇文楼",
    name_en: "Huiwen Building",
    campus: "沧海校区",
    latitude_gcj02: 22.5377,
    longitude_gcj02: 113.935,
    radius_m: 175,
    status: "listed",
  },
];

export const mockSigninQwStatus = { state: "ready", message: "" };

export const mockSigninAutomation: SigninAutomationSettings = {
  lmsAutoRadar: false,
  szuEnabled: false,
};

export const mockSigninRollcalls: RollcallItem[] = [
  {
    rollcall_id: 123,
    is_number: true,
    is_radar: false,
    course_id: 9,
    course_title: "数据库系统",
    student_rollcall_id: 1,
    type: "number",
    status: "on_call",
  },
  {
    rollcall_id: 456,
    is_number: false,
    is_radar: true,
    course_id: 10,
    course_title: "概率论与数理统计",
    student_rollcall_id: 2,
    type: "radar",
    status: "on_call",
  },
  {
    rollcall_id: 789,
    is_number: false,
    is_radar: false,
    course_id: 11,
    course_title: "大学英语",
    student_rollcall_id: 3,
    type: "qr",
    status: "on_call",
  },
  {
    rollcall_id: 102,
    is_number: false,
    is_radar: false,
    course_id: 12,
    course_title: "线性代数",
    student_rollcall_id: 4,
    type: "number",
    status: "on_call_fine",
  },
];

const now = Date.now();

export const mockSigninTimetable: TimetableCache = {
  fetched_at_ms: now,
  semester: "2026-2027-1",
  sessions: [
    {
      date: localDateKey(new Date(now)),
      course_name: "概率论与数理统计",
      building_name: "致理楼",
      building_name_en: "Zhili Building",
      classroom: "L1-203",
      start_ms: now - 10 * 60_000,
      end_ms: now + 50 * 60_000,
      semester: "2026-2027-1",
    },
    {
      date: localDateKey(new Date(now)),
      course_name: "数据结构与算法",
      building_name: "致腾楼",
      building_name_en: "Zhiteng Building",
      classroom: "102",
      start_ms: now + 90 * 60_000,
      end_ms: now + 150 * 60_000,
      semester: "2026-2027-1",
    },
  ],
};
