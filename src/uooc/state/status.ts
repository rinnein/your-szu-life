import type { CheckinState, QuizStage, UoocTask, UoocTaskStatus } from "./types";

/** 任务是否处于运行/排队（会占用并发、可暂停）。 */
export const taskActive = (task: UoocTask) => task.status === "running" || task.status === "queued";

/**
 * 后端状态 → 展示语义（文案 + 颜色）的穷尽映射。
 *
 * 颜色语义在 UOOC 内统一为：success 已完成、accent 进行中、warning 需要用户
 * 处理、danger 失败、default 未开始或不适用。各处 Chip 直接取这里的映射，
 * 不再自己挑颜色（此前同一含义在五个文件里用过三种颜色）。
 */
export type StatusTone = "default" | "accent" | "success" | "warning" | "danger" | "info";

export interface StatusView {
  label: string;
  tone: StatusTone;
}

/** 视频任务（复用后端任务状态枚举）的展示状态。 */
export const videoStatus: Record<UoocTaskStatus, StatusView> = {
  queued: { label: "等待执行", tone: "default" },
  running: { label: "刷课中", tone: "accent" },
  paused: { label: "已暂停", tone: "default" },
  stopped: { label: "已停止", tone: "default" },
  awaitingLogin: { label: "等待登录", tone: "warning" },
  systemPaused: { label: "系统暂停", tone: "default" },
  completed: { label: "已刷完", tone: "success" },
  dryRun: { label: "预览完成", tone: "success" },
  blocked: { label: "前置任务未完成", tone: "warning" },
  failed: { label: "未完成", tone: "danger" },
  awaitingDecision: { label: "等待测验处理", tone: "warning" },
};

/** 课程任务整体（任务状态 + 是否全部刷完）的展示状态。 */
export function courseTaskStatus(task: UoocTaskStatus, videosCompleted: boolean): StatusView {
  if (videosCompleted && (task === "running" || task === "queued")) {
    return { label: "视频已刷完", tone: "success" };
  }
  return videoStatus[task];
}

/** 测验阶段的展示状态。 */
export const quizStageStatus: Record<QuizStage, StatusView> = {
  idle: { label: "未开始", tone: "default" },
  preparing: { label: "处理中", tone: "accent" },
  generating: { label: "答题中", tone: "accent" },
  awaitingChoice: { label: "待答题", tone: "warning" },
  awaitingApproval: { label: "待核对", tone: "warning" },
  submitting: { label: "提交中", tone: "accent" },
  verifying: { label: "核对中", tone: "accent" },
  passed: { label: "已完成", tone: "success" },
  failed: { label: "未完成", tone: "danger" },
  manual: { label: "手动处理", tone: "warning" },
  disabled: { label: "无测验", tone: "default" },
};

/** 测验是否处于后端正在处理的阶段。 */
export function quizStageBusy(stage: QuizStage): boolean {
  return ["preparing", "generating", "submitting", "verifying"].includes(stage);
}

/** 签到状态的展示。 */
export const checkinStatus: Record<CheckinState["status"], StatusView> = {
  queued: { label: "签到排队中", tone: "accent" },
  submitting: { label: "提交签到", tone: "accent" },
  verifying: { label: "确认签到结果", tone: "accent" },
  succeeded: { label: "签到成功", tone: "success" },
  skipped: { label: "无需签到", tone: "default" },
  cancelled: { label: "已取消", tone: "default" },
  failed: { label: "签到失败", tone: "danger" },
};

/** 签到是否仍在进行（进行中不可重复发起）。 */
export const checkinOngoing = (state?: CheckinState) =>
  !!state && ["queued", "submitting", "verifying"].includes(state.status);
