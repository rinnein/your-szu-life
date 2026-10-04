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
  session: Session;
  message: string;
  lastSync?: number;
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
  problem?: number;
  status?: number;
  language?: string;
  code?: string;
  create_time?: string;
  time?: number;
  memory?: number;
  result?: unknown;
  [key: string]: unknown;
}
export interface Records {
  records: Submission[];
  uncertain: { key: string; details: { time: number; problemKey?: string } }[];
}
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
  `/oj?${new URLSearchParams(Object.entries(values).filter((v): v is [string, string] => v[1] !== undefined))}`;
