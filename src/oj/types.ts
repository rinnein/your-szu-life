import type { CompileOptions } from "./compile-options";
export type Session =
  | "disabled"
  | "connecting"
  | "ready"
  | "captchaRequired"
  | "loginFailed"
  | "networkError";
export interface Site {
  id: string;
  name: string;
  url: string;
  builtin: boolean;
  reminders: boolean;
  enabled: boolean;
  username: string;
  sessionGeneration?: number | null;
  session: Session;
  message: string;
  lastSync?: number;
  /** 本地已有该站点的班级/测验缓存（会话尚未建立时也可展示）。 */
  hasCache?: boolean;
  /** 上次登录确认的平台角色：0 管理员 / 1 教师 / 2 助教 / 3 学生。 */
  permission?: number | null;
}
export interface Notice {
  id: string;
  siteId: string;
  account: string;
  contestId: string;
  title: string;
  message: string;
  timestamp: number;
  read: boolean;
}
export interface Snapshot {
  sites: Site[];
  notices: Notice[];
  companionPort: number;
  desktop: boolean;
  companionEnabled?: boolean;
  companionStatus?: { connected: boolean; active: boolean; message: string };
}
export interface Group {
  id: string;
  name: string;
}
export interface Contest {
  id: string;
  groupId: string;
  title: string;
  description: string;
  begin: number;
  end: number;
  now: number;
  isExam: boolean;
}
/** 首页待办日历使用的扁平截止项。 */
export interface OjDeadline {
  siteId: string;
  siteName: string;
  groupId: string;
  id: string;
  title: string;
  begin: number;
  end: number;
  now: number;
  isExam: boolean;
  accepted: number;
  total: number;
  hasProgress: boolean;
}
/** OJ 班级/测验缓存，键为站点 ID 或 `site:group`。 */
export interface OjLists {
  groups: Record<string, Group[]>;
  contests: Record<string, Contest[]>;
}
export interface Problem {
  id: string;
  displayLabel: string;
  kind: number;
  title: string;
  description: string;
  input: string;
  output: string;
  hint: string;
  timeLimit: number;
  memoryLimit: number;
  samples: { input: string; output: string }[];
  choices: string[];
  multiple: boolean;
  blanks: number;
  languages: string[];
  templates: Record<string, string>;
  score?: number;
  ioMode: string;
}
export interface Draft {
  code: string;
  language: string;
  answer: string | number[] | string[];
  options: CompileOptions | null;
  revision: number;
}
export interface Preferences {
  language: string;
  optionsByLanguage: Record<string, CompileOptions>;
  revision: number;
}
export interface Paper {
  contest: Contest;
  problems: Problem[];
  drafts: Record<string, Draft>;
  account: string;
  epoch: number;
  writable: boolean;
  submitted: Record<string, unknown> | null;
  sourceUrl: string;
  preferences?: Preferences;
}
export interface Submission {
  id: number | string;
  problem?: number | string | { id: number | string };
  status?: number;
  language?: string;
  code?: string;
  create_time?: string;
  time?: number | string;
  memory?: number;
  result?: unknown;
  [key: string]: unknown;
}
export interface Records {
  records: Submission[];
  uncertain: { key: string; details: { time: number; problemKey?: string } }[];
}
export interface ProgressSummary {
  accepted: number;
  total: number;
}
export interface Ranking {
  count: number;
  rows: {
    rank: number;
    username: string;
    name: string;
    nickname: string;
    accepted: number;
    penalty: string;
    problems: Record<string, { accepted: boolean; errors: number; seconds: number | null }>;
  }[];
}
export interface ReviewUser {
  id: string;
  username: string;
  name: string;
  nickname: string;
}
/** 助教工作台的管理测验列表项。 */
export interface ManagedContest {
  id: string;
  groupId: string;
  groupName: string;
  title: string;
  description: string;
  begin: number;
  end: number;
  isExam: boolean;
  scoredStatus: string;
  scoreStartTime?: number | null;
  totalScore?: number | null;
}
export interface ManagedContests {
  rows: ManagedContest[];
  now: number;
}
export interface ManagedContestDetail extends ManagedContest {
  problems: Problem[];
  now: number;
}
export interface ReviewSummary {
  id: string;
  user: ReviewUser;
  totalScore: number;
  judged: boolean;
  checked: boolean;
}
export interface ReviewList {
  count: number;
  rows: ReviewSummary[];
}
export interface ReviewProblem {
  id: string;
  displayLabel: string;
  kind: number;
  title: string;
  description: string;
  hint: string;
  maxScore: number;
  score: number;
  submissionId?: string | null;
  answer: string | number[] | string[] | null;
  choices: string[];
  multiple: boolean;
  blanks: number;
  code?: string | null;
  language?: string | null;
  status?: number | null;
  comment: string;
  referenceAnswer?: string | number[] | string[] | null;
  judge?: Submission | null;
  loadError?: string | null;
}
export interface ReviewDetail {
  contest: ManagedContest;
  submissionId: string;
  user: ReviewUser;
  totalScore: number;
  judged: boolean;
  checked: boolean;
  problems: ReviewProblem[];
  warnings: string[];
}
export interface ProblemSubmissionRow {
  id: string;
  user: ReviewUser;
  language: string;
  status?: number | null;
  createTime?: unknown;
  cpuTime?: number | null;
  realTime?: number | null;
  memory?: number | null;
  canViewDetail: boolean;
}
export interface ProblemSubmissions {
  count: number;
  rows: ProblemSubmissionRow[];
}
export interface TaSubmission {
  submission: Submission;
  expected: string[];
  samples: string[];
  warnings: string[];
}
export interface ReviewScores {
  coding: Record<string, Record<string, number>>;
  codeFill: Record<string, Record<string, number>>;
  qa: Record<string, Record<string, number>>;
  choice: Record<string, number>;
  fillIn: Record<string, number>;
}
export interface WriteResult {
  saved: boolean;
  uncertain: boolean;
  message: string;
}
export interface SaveReviewResult extends WriteResult {
  detail?: ReviewDetail | null;
}
export const roleName: Record<number, string> = {
  0: "管理员",
  1: "教师",
  2: "助教",
  3: "学生",
};
export const canReview = (site?: Pick<Site, "permission"> | null) =>
  site?.permission != null && site.permission <= 2;
export const scoredStatusName: Record<string, string> = {
  waiting: "等待机评",
  running: "机评中",
  done: "已完成",
  error: "机评失败",
};
export const taPhase = (contest: { begin: number; end: number }, now: number) =>
  now < contest.begin ? 0 : now < contest.end ? 1 : 2;
export const taPhaseName = ["未开始", "进行中", "已结束"] as const;
export const reviewTag = (row: Pick<ReviewSummary, "judged" | "checked">) =>
  row.judged ? (row.checked ? "已阅" : "未人工批阅") : "尚未自动评分";
export const reviewTagColor = (row: Pick<ReviewSummary, "judged" | "checked">) =>
  row.judged ? (row.checked ? "success" : "warning") : "default";
export const problemKey = (p: Problem) => `${p.kind}:${p.id}`;
export const kindName = ["编程题", "选择题", "填空题", "问答题", "代码补全"];
export const languageName: Record<string, string> = {
  c: "C",
  cpp: "C++",
  java: "Java",
  py: "Python 3",
  go: "Go",
  js: "JavaScript",
  c_asan: "C Asan",
  cpp_asan: "C++ Asan",
};
export const statusName: Record<number, string> = {
  [-3]: "格式错误",
  [-2]: "编译错误",
  [-1]: "答案错误",
  0: "通过",
  1: "CPU 超时",
  2: "运行超时",
  3: "内存超限",
  4: "运行异常",
  5: "系统错误",
  6: "等待判题",
  7: "判题中",
  8: "部分正确",
};
export const sessionName: Record<Session, string> = {
  disabled: "等待连接",
  connecting: "连接中",
  ready: "已登录",
  captchaRequired: "需要验证码",
  loginFailed: "登录失败",
  networkError: "网络不可达",
};
export const phase = (c: Contest, now = c.now) =>
  now < c.begin ? "未开始" : now < c.end ? "进行中" : "已结束";
export const route = (values: Record<string, string | undefined>) =>
  `/study/oj?${new URLSearchParams(Object.entries(values).filter((v): v is [string, string] => v[1] !== undefined))}`;
