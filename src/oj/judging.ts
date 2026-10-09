import type { Submission } from "./types";

type Color = "success" | "danger" | "warning" | "accent" | "default";
export const verdicts: Record<number, { code: string; label: string; color: Color }> = {
  [-3]: { code: "PE", label: "格式错误", color: "danger" },
  [-2]: { code: "CE", label: "编译错误", color: "warning" },
  [-1]: { code: "WA", label: "答案错误", color: "danger" },
  0: { code: "AC", label: "通过", color: "success" },
  1: { code: "TLE", label: "CPU 超时", color: "warning" },
  2: { code: "TLE", label: "运行超时", color: "warning" },
  3: { code: "MLE", label: "内存超限", color: "warning" },
  4: { code: "RE", label: "运行错误", color: "danger" },
  5: { code: "SE", label: "系统错误", color: "danger" },
  6: { code: "Pending", label: "等待判题", color: "default" },
  7: { code: "Judging", label: "判题中", color: "accent" },
  8: { code: "PA", label: "部分正确", color: "warning" },
};
export const isPending = (s: Submission) => s.status == null || s.status === 6 || s.status === 7;
export const submissionProblem = (s: Submission) =>
  String(typeof s.problem === "object" ? s.problem.id : (s.problem ?? ""));
export const newestFirst = (a: Submission, b: Submission) => {
  const time = (s: Submission) => {
    const value = s.create_time ?? s.time;
    return typeof value === "number" ? value : Date.parse(value ?? "") || 0;
  };
  return time(b) - time(a) || Number(b.id) - Number(a.id);
};
/** A solved problem stays AC; otherwise show the latest attempt, never the best failure. */
export function problemVerdict(records: Submission[]): number | undefined {
  if (records.some((s) => s.status === 0)) return 0;
  const latest = [...records].sort(newestFirst)[0];
  return latest ? (latest.status ?? 6) : undefined;
}
export function submissionDate(s: Submission): string {
  const value = s.create_time ?? s.time;
  if (value == null) return "";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "" : date.toLocaleString();
}
const object = (value: unknown): Record<string, unknown> =>
  value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
const number = (v: unknown) =>
  typeof v === "number" && Number.isFinite(v) && v >= 0 ? v : undefined;
export function judgeDetails(s: Submission) {
  const result = object(s.result);
  const cases = (Array.isArray(result.data) ? result.data : []).map((item, index) => {
    const row = object(item);
    return {
      id: String(row.test_case ?? index + 1),
      status: typeof row.result === "number" ? row.result : undefined,
      cpu: number(row.cpu_time),
      real: number(row.real_time),
      memory: number(row.memory),
    };
  });
  const maximum = (values: (number | undefined)[]) => {
    const present = values.filter((v): v is number => v !== undefined);
    return present.length ? Math.max(...present) : undefined;
  };
  const lint = object(s.lint);
  return {
    cases,
    cpu: maximum([number(s.cpu_time), ...cases.map((c) => c.cpu)]),
    memory: maximum([number(s.memory), ...cases.map((c) => c.memory)]),
    message:
      typeof s.result === "string"
        ? s.result
        : typeof result.error === "string"
          ? result.error
          : undefined,
    lintScore: number(lint.mark),
    lintMessage:
      [lint.result, lint.err, lint.message]
        .filter((value): value is string => typeof value === "string")
        .join("\n") || undefined,
  };
}
export const milliseconds = (v?: number) => (v === undefined ? "未提供" : `${v} ms`);
export const megabytes = (v?: number) =>
  v === undefined ? "未提供" : `${(v / 1024 / 1024).toFixed(2)} MB`;
