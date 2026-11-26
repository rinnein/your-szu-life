/** 与 `szu-enrollment::enrollment::evaluation::model` 逐字段对应。 */

export type TargetStatus = "pending" | "done" | "expired" | "unsupported";

export interface EvaluationTerm {
  id: number;
  name: string;
}

export interface ChoiceOption {
  label: string;
  valueLow: string;
  valueHigh: string;
  valueNormal: string;
}

export interface EvaluationQuestion {
  subId: number;
  subName: string;
  kind: string;
  choices: ChoiceOption[];
}

export interface EvaluationRecord {
  scId: number;
  courseName: string;
  courseNo: string;
  employeeName: string;
  className: string;
  currentTimes: number;
  limitedTimes: number;
  expiredDate: string | null;
  participate: boolean;
}

export interface EvaluationTarget {
  id: string;
  targetId: number;
  targetName: string;
  termName: string;
  expiredDate: string | null;
  status: TargetStatus;
  message: string | null;
  scoreType: number;
  commentRequired: number;
  staySeconds: number;
  records: EvaluationRecord[];
  questions: EvaluationQuestion[];
}

export interface EvaluationOverview {
  fetchedAt: number;
  term: EvaluationTerm | null;
  comment: string;
  pendingTargets: number;
  pendingRecords: number;
  expiredRecords: number;
  targets: EvaluationTarget[];
}

export interface EvaluationConfig {
  comment: string;
}

export interface RunFailure {
  targetName: string;
  courseName?: string | null;
  message: string;
}

export interface EvaluationRunSummary {
  submittedTargets: number;
  submittedRecords: number;
  failures: RunFailure[];
}

export interface TargetProgress {
  index: number;
  total: number;
  targetName: string;
  state: "started" | "ok" | "skipped" | "failed";
  message: string | null;
}

export type EvaluationRunEvent =
  | { type: "target"; data: TargetProgress }
  | { type: "waiting"; data: { seconds: number } }
  | { type: "done"; data: { summary: EvaluationRunSummary; dryRun: boolean } };

export interface RunOutcome {
  dryRun: boolean;
  summary: EvaluationRunSummary;
  plans: unknown[];
}
