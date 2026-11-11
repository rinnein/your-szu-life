import type {
  CourseProgress,
  ProgressEntry,
  ProgressMetric,
  UoocSnapshot,
  UoocTask,
} from "../../uooc/types";
import { mockProfile } from "./auth";

const okMetric = (percent: number, count: number, total: number): ProgressMetric => ({
  percent,
  count,
  total,
});

const courseProgress: CourseProgress = {
  totalPercent: 82,
  video: okMetric(95, 19, 20),
  test: okMetric(75, 6, 8),
  homework: okMetric(80, 4, 5),
  signin: okMetric(100, 6, 6),
  discussion: okMetric(60, 3, 5),
  weightedPercent: 84,
  signinWeight: 10,
  signinTime: "2026-10-30 08:05",
  attendance: "complete",
};

const progressEntry = (data: CourseProgress | null): ProgressEntry => ({
  data,
  updatedAtMs: Date.now() - 60_000,
  refreshing: false,
  error: null,
});

const course = (id: string, title: string) => ({
  id,
  title,
  semester: "2026-2027学年第一学期",
  organization: "深圳大学",
  weeks: "第1-16周",
  status: "进行中",
});

export const mockUoocCourses = [
  course("c-1001", "人工智能导论"),
  course("c-1002", "数据结构与算法"),
];

export const mockUoocTasks: UoocTask[] = [
  {
    course: mockUoocCourses[0],
    accountId: "demo-account",
    desiredRunning: true,
    status: "completed",
    message: "全部章节已完成",
    videos: [
      {
        resourceId: "r-1",
        title: "1.1 什么是人工智能",
        status: "completed",
        position: 100,
        duration: 720,
        message: "完成",
        taskType: "video",
        taskId: "t-1",
      },
      {
        resourceId: "r-2",
        title: "1.2 机器学习概览",
        status: "completed",
        position: 100,
        duration: 845,
        message: "完成",
        taskType: "video",
        taskId: "t-2",
      },
    ],
    generation: 1,
  },
];

export const mockUoocSnapshot: UoocSnapshot = {
  revision: 1,
  session: "ready",
  user: { id: mockProfile.studentId, name: mockProfile.name ?? "演示同学" },
  courses: mockUoocCourses,
  settings: {
    concurrency: 2,
    reportIntervalSec: 30,
    speedMultiplier: 1,
    dryRun: false,
    discussionEnabled: true,
    discussionContent: "演示模式的讨论区发言内容。",
    discussionIntervalMin: 15,
  },
  tasks: mockUoocTasks,
  message: "",
  logs: [],
  progress: {
    "c-1001": progressEntry(courseProgress),
    "c-1002": progressEntry({ ...courseProgress, totalPercent: 45, video: okMetric(45, 9, 20) }),
  },
};

const iso = (days: number): string => {
  const date = new Date();
  date.setDate(date.getDate() + days);
  return date.toISOString();
};

export const mockUoocAssessments = {
  exams: [
    {
      id: "exam-1",
      title: "人工智能导论 期末考试",
      cid: "c-1001",
      courseName: "人工智能导论",
      startTime: iso(5),
      endTime: iso(7),
      limitTime: 120,
      status: 1,
      statusText: "未开始",
      canDo: false,
      canView: false,
      chance: 1,
      submitCount: 0,
      score: null,
      taskId: "task-exam-1",
    },
  ],
  homeworks: [
    {
      id: "hw-1",
      title: "数据结构与算法 单元作业",
      cid: "c-1002",
      courseName: "数据结构与算法",
      startTime: iso(-3),
      endTime: iso(7),
      status: 1,
      statusText: "进行中",
      canDo: true,
      canView: true,
      chance: 3,
      submitCount: 0,
      score: null,
      taskId: "task-hw-1",
      hasEval: false,
    },
  ],
};
