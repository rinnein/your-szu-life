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
  qaServerUrl?: string;
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
  taskType?: "video" | "quiz" | string;
  taskId?: string | null;
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
export type AiSessionEntry = {
  batchIndex: number;
  questionIds: string[];
  questionTitles: string[];
  model: string;
  prompt: string;
  response: string;
  answers: Record<string, string[]>;
  status: "pending" | "completed" | "error";
  error?: string | null;
};
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
  aiSessions?: AiSessionEntry[];
  studentAnswers?: QuizAnswers;
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
  batchSize?: number;
  reasoningEffort?: string;
  includeReasoning?: boolean;
  webSearch?: boolean;
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
  const items: CourseTaskItem[] = videos.map((v) => {
    const isQuiz = v.taskType === "quiz" || (!v.duration && v.title.includes("测验"));
    if (isQuiz) {
      const isCompleted = v.status === "completed";
      const itemQuiz: QuizState = {
        accountId: quiz?.accountId ?? "",
        cid: quiz?.cid ?? "",
        taskId: v.taskId ?? "",
        resourceId: v.resourceId,
        title: v.title,
        mode: quiz?.mode ?? "supervised",
        stage: isCompleted
          ? "passed"
          : quiz?.resourceId === v.resourceId
            ? quiz.stage
            : "awaitingApproval",
        revision: quiz?.resourceId === v.resourceId ? quiz.revision : 1,
        questions: quiz?.resourceId === v.resourceId ? quiz.questions : [],
        answers: quiz?.resourceId === v.resourceId ? quiz.answers : {},
        reviews: quiz?.resourceId === v.resourceId ? quiz.reviews : [],
        message: isCompleted
          ? "当前测验已由服务端确认完成"
          : quiz?.resourceId === v.resourceId
            ? quiz.message
            : "",
        gated: quiz?.gated ?? false,
        videoComplete: isCompleted,
        passed: isCompleted ? [v.resourceId] : (quiz?.passed ?? []),
      };
      return {
        id: `quiz-${v.resourceId}`,
        kind: "quiz",
        quiz: itemQuiz,
      };
    }
    return {
      id: `video-${v.resourceId}`,
      kind: "video",
      video: v,
    };
  });

  if (
    !quiz ||
    quiz.stage === "disabled" ||
    (!quiz.taskId && !quiz.title && quiz.stage === "idle") ||
    items.some(
      (item) =>
        item.kind === "quiz" &&
        (item.quiz.resourceId === quiz.resourceId || item.quiz.taskId === quiz.taskId),
    )
  ) {
    return items;
  }

  const quizItem: CourseTaskItem = {
    id: `quiz-${quiz.cid}-${quiz.resourceId || quiz.taskId || "current"}`,
    kind: "quiz",
    quiz,
  };

  if (quiz.videoComplete) {
    return [...items, quizItem];
  }

  if (quiz.gated) {
    let lastCompletedIdx = -1;
    for (let i = items.length - 1; i >= 0; i--) {
      const item = items[i];
      const isDone =
        item.kind === "video" ? item.video.status === "completed" : item.quiz.stage === "passed";
      if (isDone) {
        lastCompletedIdx = i;
        break;
      }
    }
    if (lastCompletedIdx === -1) {
      return [quizItem, ...items];
    }
    const result = [...items];
    result.splice(lastCompletedIdx + 1, 0, quizItem);
    return result;
  }

  const firstIncompleteIdx = items.findIndex((item) => {
    return item.kind === "video" ? item.video.status !== "completed" : item.quiz.stage !== "passed";
  });
  if (firstIncompleteIdx !== -1) {
    const result = [...items];
    result.splice(firstIncompleteIdx, 0, quizItem);
    return result;
  }

  return [...items, quizItem];
}

export type LocalBankAnswer = {
  id?: number;
  values: unknown[];
  isCorrect: boolean;
  source: string;
  confidence: number;
};

export type LocalBankQuestion = {
  cid: string;
  tid: string;
  questionId: string;
  questionType: string;
  type?: string;
  stem?: string | null;
  options?: { id: string; text: string }[] | null;
  blanks?: number | null;
  answers?: LocalBankAnswer[];
  correctAnswers?: string[];
  uploaded: boolean;
  updatedAtMs: number;
  courseName?: string | null;
  taskName?: string | null;
};
