import type { Course, Snapshot, Task } from "../../enrollment/model";

const course = (
  id: string,
  code: string,
  name: string,
  teacher: string,
  remaining: number,
): Course => ({
  id,
  code,
  name,
  teacher,
  category: "专业选修",
  credits: 3,
  schedule: "周一 3-4节; 周三 5-6节",
  capacity: 120,
  remaining,
  campus: "粤海",
  mainSelect: false,
  hasTest: false,
  testIds: [],
});

export const mockEnrollmentCourses: Course[] = [
  course("e-1", "CS301", "编译原理", "演示教师甲", 12),
  course("e-2", "CS302", "计算机网络", "演示教师乙", 45),
  course("e-3", "MA210", "概率论与数理统计", "演示教师丙", 0),
];

export const mockEnrollmentTasks: Task[] = [];

/**
 * 顶栏「学校信息」条用的班次数据（`batchInfo` action 的返回）。
 * 页面挂载后立刻请求，缺了会弹「操作失败」。
 */
export const mockEnrollmentBatches = [
  {
    code: "2026-2027-1",
    name: "2026-2027学年第一学期",
    typeName: "正常选课",
    tacticName: "先到先得",
    beginTime: "2026-09-01 08:00",
    endTime: "2026-09-07 23:59",
    active: "1",
  },
  {
    code: "2026-2027-1-2",
    name: "补选阶段",
    typeName: "补退选",
    tacticName: "先到先得",
    beginTime: "2026-09-08 08:00",
    endTime: "2026-09-14 23:59",
    active: "0",
  },
];

export const mockEnrollmentSnapshot: Snapshot = {
  mode: "campus",
  context: { student: "20250001", term: "2026-2027-1", batch: "2026级", campus: "粤海" },
  loggedIn: true,
  authentication: "authenticated",
  admission: "ready",
  message: "",
  courses: mockEnrollmentCourses,
  selected: [mockEnrollmentCourses[0]],
  tasks: mockEnrollmentTasks,
  pins: [],
  updatedAt: Date.now() - 60_000,
  complete: true,
  running: false,
  academic: null,
  query: { category: "", keyword: "", page: 1 },
};
