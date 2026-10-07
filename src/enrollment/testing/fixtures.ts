import type { Academic, Course, Snapshot } from "../model";
const rows = Array.from({ length: 48 }, (_, i) => ({
  KCH: `GE${String(i + 1).padStart(3, "0")}`,
  KCM:
    [
      "中国历史与文化",
      "艺术与社会",
      "科技伦理与社会责任",
      "跨学科研究方法与复杂系统思维（跨院系长名称课程）",
    ][i % 4] + (i > 3 ? ` ${i + 1}` : ""),
  XF: (i % 3) + 1,
  KCLBDM_DISPLAY: i % 2 ? "人文素养" : "科学素养",
  KCXZDM_DISPLAY: "选修",
  SFTG: i % 3 === 0 ? "1" : "2",
  SFXK: i % 7 === 0 ? "1" : "0",
  CJ: i % 3 === 0 ? "85" : null,
}));
export const academicFixture: Academic = {
  summary: [
    { PYFADM: "plan-2026", PYFAMC: "计算机科学与技术 · 本科培养方案", YQXF: 160, WCXF: 108 },
  ],
  modules: [
    { module: { KZH: "general", KZM: "通识教育", YQXF: 40, WCXF: 26 }, courses: [] },
    {
      module: { KZH: "culture", FKZH: "general", KZM: "人文与艺术素养", YQXF: 8, WCXF: 6 },
      courses: rows,
    },
    {
      module: { KZH: "science", FKZH: "general", KZM: "科学思维与创新", YQXF: 8, WCXF: 2 },
      courses: [{ ...rows[0], XF: 4, SFTG: "2" }],
    },
    {
      module: { KZH: "major", KZM: "专业教育", YQXF: 90, WCXF: 70 },
      courses: [
        { KCH: "CS101", KCM: "数据结构", XF: 4, SFTG: "1", CJ: "92", KCLBDM_DISPLAY: "专业必修" },
        { KCH: "CS201", KCM: "操作系统", XF: 4, SFTG: "2", KCLBDM_DISPLAY: "专业必修" },
      ],
    },
    {
      module: { KZH: "practice", KZM: "实践与毕业设计", YQXF: 30, WCXF: 12 },
      courses: [{ KCH: "PR001", KCM: "毕业设计", XF: 12, SFTG: "2", KCLBDM_DISPLAY: "实践课程" }],
    },
  ],
  calculatedAt: "2026-10-07 10:30",
  updatedAt: 1791340200000,
};
export const courseFixture: Course = {
  id: "class-new",
  name: "中国历史与文化",
  code: "GE001",
  teacher: "张老师",
  category: "XGXK",
  credits: 2,
  schedule: "2-16周 星期一 3-4节 粤海校区教学楼",
  capacity: 80,
  remaining: 3,
  campus: "01",
  mainSelect: true,
  hasTest: false,
  testIds: [],
};
export const snapshotFixture: Snapshot = {
  mode: "campus",
  context: { student: "synthetic", term: "2026-2027-1", batch: "batch-demo", campus: "01" },
  loggedIn: true,
  message: "",
  courses: [
    courseFixture,
    {
      ...courseFixture,
      id: "class-2",
      name: "跨学科研究方法与复杂系统思维（跨院系长名称课程）",
      code: "GE004",
      teacher: "李老师",
      schedule: "2-16周(双) 星期五 11-12节 丽湖校区",
      remaining: 0,
    },
    {
      ...courseFixture,
      id: "class-3",
      name: "数据结构",
      code: "CS101",
      credits: 4,
      category: "FANKC",
    },
    {
      ...courseFixture,
      id: "class-4",
      name: "大学体育：羽毛球",
      code: "PE001",
      category: "TYKC",
      teacher: "王老师",
    },
  ],
  selected: [{ ...courseFixture, id: "selected-1", name: "操作系统", code: "CS201", credits: 4 }],
  tasks: [],
  pins: [],
  updatedAt: 1791340200000,
  complete: false,
  running: false,
  academic: academicFixture,
  query: { category: "XGXK", keyword: "", page: 0 },
};
