export type UoocCourse = {
  id: string;
  title: string;
  semester: string;
  organization: string;
  weeks: string;
  endDate?: string;
};
export type UoocSettings = {
  concurrency: number;
  reportIntervalSec: number;
  speedMultiplier: number;
  dryRun: boolean;
  discussionEnabled?: boolean;
  discussionContent?: string;
  discussionIntervalMin?: number;
};
export type UoocTaskStatus =
  | "queued"
  | "running"
  | "paused"
  | "stopped"
  | "awaitingLogin"
  | "systemPaused"
  | "completed"
  | "dryRun"
  | "blocked"
  | "failed"
  | "awaitingDecision";
export type UoocVideo = {
  resourceId: string;
  title: string;
  status: UoocTaskStatus;
  position: number;
  duration: number | null;
  message: string;
};
export type UoocTask = {
  course: UoocCourse;
  accountId: string;
  desiredRunning: boolean;
  status: UoocTaskStatus;
  message: string;
  videos: UoocVideo[];
  generation: number;
};
export type UoocSnapshot = {
  revision: number;
  session: "checking" | "ready" | "loginRequired" | "networkError" | "loggingIn";
  user: { id: string; name: string } | null;
  courses: UoocCourse[];
  settings: UoocSettings;
  tasks: UoocTask[];
  message: string;
  logs: string[];
  progress?: Record<string, ProgressEntry>;
  checkins?: Record<string, CheckinState>;
  quizzes?: Record<string, QuizState>;
  discussions?: Record<string, DiscussionCourseState>;
  discussionCooldownUntilMs?: number | null;
};
export type DiscussionCourseState = {
  status: "idle" | "waitingCooldown" | "eligible" | "sending" | "completed" | "skipped" | "failed";
  message: string;
  updatedAtMs: number;
  lastTid?: string | null;
  lastSentAtMs?: number | null;
};
export type ProgressMetric = {
  percent: number | null;
  count: number | null;
  total: number | null;
  weight?: number | null;
  score?: number | null;
  averageScore?: number | null;
};
export type Attendance = "notRequired" | "complete" | "today" | "needed" | "unknown";
export type CourseProgress = {
  totalPercent: number | null;
  video: ProgressMetric;
  test: ProgressMetric;
  homework: ProgressMetric;
  signin: ProgressMetric;
  discussion?: ProgressMetric;
  exam?: ProgressMetric;
  weightedPercent?: number | null;
  weightedPartial?: boolean;
  weightTotal?: number | null;
  score?: number | null;
  weightedScore?: number | null;
  videoMinutes?: number | null;
  discussionPosts?: number | null;
  discussionReplies?: number | null;
  signinWeight: number | null;
  signinTime: string | null;
  attendance: Attendance;
};
export type ProgressEntry = {
  data: CourseProgress | null;
  updatedAtMs: number | null;
  refreshing: boolean;
  error: string | null;
};
export type CheckinState = {
  id: string;
  status: "queued" | "submitting" | "verifying" | "succeeded" | "skipped" | "cancelled" | "failed";
  source: "automatic" | "manual";
  message: string;
  updatedAtMs: number;
};
export const attendanceText: Record<Attendance, string> = {
  notRequired: "无需签到",
  complete: "签到已完成",
  today: "今日已签到",
  needed: "待签到",
  unknown: "签到状态未知",
};
export const checkinActive = (state?: CheckinState) =>
  !!state && ["queued", "submitting", "verifying"].includes(state.status);
export const taskStatus: Record<UoocTaskStatus, string> = {
  queued: "等待执行",
  running: "运行中",
  paused: "已暂停",
  stopped: "已停止",
  awaitingLogin: "等待登录",
  systemPaused: "系统暂停",
  completed: "已完成",
  dryRun: "预览完成",
  blocked: "前置任务未完成",
  failed: "未完成",
  awaitingDecision: "等待测验处理",
};
export function newerSnapshot(previous: UoocSnapshot | undefined, next: UoocSnapshot) {
  return !previous || next.revision >= previous.revision ? next : previous;
}
export function isUoocSnapshot(value: unknown): value is UoocSnapshot {
  return (
    !!value &&
    typeof value === "object" &&
    "revision" in value &&
    typeof value.revision === "number" &&
    "courses" in value &&
    Array.isArray(value.courses) &&
    "tasks" in value &&
    Array.isArray(value.tasks)
  );
}

export type QuizMode = "yolo" | "approval" | "supervised";
export type QuizStage =
  | "idle"
  | "preparing"
  | "generating"
  | "awaitingChoice"
  | "awaitingApproval"
  | "submitting"
  | "verifying"
  | "passed"
  | "failed"
  | "manual"
  | "disabled";
export type QuizQuestion = {
  id: string;
  text: string;
  kind: "single" | "multiple" | "text";
  options: { id: string; text: string }[];
  blanks: number;
  images: string[];
};
export type QuizAnswers = Record<string, string[]>;
export type QuizState = {
  accountId: string;
  cid: string;
  taskId: string;
  resourceId: string;
  title: string;
  mode: QuizMode;
  stage: QuizStage;
  revision: number;
  questions: QuizQuestion[];
  answers: QuizAnswers;
  reviews: {
    questionId: string;
    answers: string[];
    probabilities: Record<string, number>;
    message: string;
  }[];
  message: string;
  gated: boolean;
  videoComplete: boolean;
  passed: string[];
};
export type AiProvider = { baseUrl: string; model: string; hasKey: boolean };
export type AiConnectionProvider = { baseUrl: string; hasKey: boolean };
export type AiConnections = { main: AiConnectionProvider; jev: AiConnectionProvider };
export type AiConfig = {
  main: AiProvider;
  jev: AiProvider;
  jevEnabled: boolean;
  confidence: number;
  defaultMode: QuizMode;
};

export type CourseTaskItem =
  | {
      id: string;
      kind: "video";
      video: UoocVideo;
    }
  | {
      id: string;
      kind: "quiz";
      quiz: QuizState;
    };

export function isCourseTaskCompleted(task: UoocTask, progressEntry?: ProgressEntry): boolean {
  return (
    task.status === "completed" ||
    progressEntry?.data?.video.percent === 100 ||
    (task.videos.length > 0 && task.videos.every((v) => v.status === "completed"))
  );
}

export function buildCourseTaskItems(videos: UoocVideo[] = [], quiz?: QuizState): CourseTaskItem[] {
  const videoItems: CourseTaskItem[] = videos.map((v) => ({
    id: `video-${v.resourceId}`,
    kind: "video",
    video: v,
  }));

  if (
    !quiz ||
    quiz.stage === "disabled" ||
    (!quiz.taskId && !quiz.title && quiz.stage === "idle")
  ) {
    return videoItems;
  }

  const quizItem: CourseTaskItem = {
    id: `quiz-${quiz.cid}-${quiz.resourceId || quiz.taskId || "current"}`,
    kind: "quiz",
    quiz,
  };

  if (quiz.videoComplete) {
    return [...videoItems, quizItem];
  }

  if (quiz.gated) {
    let lastCompletedIdx = -1;
    for (let i = videos.length - 1; i >= 0; i--) {
      if (videos[i].status === "completed") {
        lastCompletedIdx = i;
        break;
      }
    }
    if (lastCompletedIdx === -1) {
      return [quizItem, ...videoItems];
    }
    const result = [...videoItems];
    result.splice(lastCompletedIdx + 1, 0, quizItem);
    return result;
  }

  const firstIncompleteIdx = videos.findIndex((v) => v.status !== "completed");
  if (firstIncompleteIdx !== -1) {
    const result = [...videoItems];
    result.splice(firstIncompleteIdx, 0, quizItem);
    return result;
  }

  return [...videoItems, quizItem];
}
