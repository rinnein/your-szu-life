import type { Contest, Group, Records, Snapshot, Submission } from "../../oj/types";
import { mockProfile } from "./auth";

const site = (id: string, name: string, url: string): Snapshot["sites"][number] => ({
  id,
  name,
  url,
  builtin: true,
  reminders: true,
  enabled: true,
  username: mockProfile.studentId,
  sessionGeneration: 1,
  session: "ready",
  message: "",
  lastSync: Date.now() - 120_000,
  hasCache: true,
});

export const mockOjSnapshot: Snapshot = {
  sites: [site("npuoj", "校 OJ", "https://oj.example.edu.cn")],
  notices: [],
  companionPort: 0,
  desktop: true,
  companionEnabled: false,
  companionStatus: { connected: false, active: false, message: "" },
};

export const mockOjGroups: Group[] = [
  { id: "g-1", name: "2026 级程序设计基础" },
  { id: "g-2", name: "算法竞赛训练营" },
];

export const mockOjContests: Contest[] = [
  {
    id: "ct-1",
    groupId: "g-1",
    title: "第七周周赛",
    description: "模拟演示用比赛",
    begin: Date.now() - 3 * 86_400_000,
    end: Date.now() - 2 * 86_400_000,
    now: Date.now(),
    isExam: false,
  },
  {
    id: "ct-2",
    groupId: "g-1",
    title: "期中上机考试",
    description: "模拟演示用考试",
    begin: Date.now() + 2 * 86_400_000,
    end: Date.now() + 2 * 86_400_000 + 2 * 3_600_000,
    now: Date.now(),
    isExam: true,
  },
];

const submission = (id: number, problem: number, status: number, language: string): Submission => ({
  id,
  problem,
  status,
  language,
  create_time: new Date(Date.now() - id * 3_600_000).toISOString().replace("T", " "),
  time: 120 + id * 13,
  memory: 1024 * (128 + id * 8),
});

export const mockOjRecords: Records = {
  records: [
    submission(1013, 1002, 0, "C++"),
    submission(1012, 1001, 0, "C++"),
    submission(1011, 1003, 5, "Python"),
    submission(1010, 1001, 4, "Java"),
    submission(1009, 1004, 6, "C"),
  ],
  uncertain: [],
};

export const mockOjProgress = { accepted: 118, total: 240 };
