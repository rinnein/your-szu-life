import type { ScheduleCategory } from "../../home-schedule/types";
import { mockProfile } from "./auth";

/** 与 src/home-schedule/use-home-deadlines.ts 的 TodoItem 结构对应。 */
export type MockTodoItem = {
  id: string;
  kind: ScheduleCategory;
  title: string;
  subtitle?: string | null;
  dueAtMs: number;
  route: string;
  externalUrl?: string | null;
  urgent: boolean;
};

const daysFromNow = (days: number): number => {
  const date = new Date();
  date.setDate(date.getDate() + days);
  date.setHours(23, 59, 0, 0);
  return date.getTime();
};

export const mockTodoItems: MockTodoItem[] = [
  {
    id: `lms-homework-9001-${mockProfile.studentId}`,
    kind: "lms-homework",
    title: "第五周作业：二叉树",
    subtitle: "数据结构与算法",
    dueAtMs: daysFromNow(2),
    route: "/study/academic",
    externalUrl: null,
    urgent: false,
  },
  {
    id: `uooc-exam-1-${mockProfile.studentId}`,
    kind: "uooc-exam",
    title: "人工智能导论 期末考试",
    subtitle: "限时 120 分钟",
    dueAtMs: daysFromNow(5),
    route: "/study/uooc",
    externalUrl: null,
    urgent: false,
  },
  {
    id: `oj-quiz-ct-2-${mockProfile.studentId}`,
    kind: "oj-quiz",
    title: "期中上机考试",
    subtitle: "校 OJ",
    dueAtMs: daysFromNow(2),
    route: "/study/oj",
    externalUrl: null,
    urgent: true,
  },
  {
    id: `uooc-homework-1-${mockProfile.studentId}`,
    kind: "uooc-homework",
    title: "数据结构与算法 单元作业",
    subtitle: "优课联盟",
    dueAtMs: daysFromNow(7),
    route: "/study/uooc",
    externalUrl: null,
    urgent: false,
  },
];
