import { useEffect, useMemo, useState } from "react";
import { Button, Card, LoadingIndicator, Progress } from "material/react";
import { ActionMenu, Banner, Chip, Disclosure } from "../../ui";
import {
  LuCheck,
  LuCircleHelp,
  LuEllipsisVertical,
  LuMessageCircle,
  LuPause,
  LuPlay,
} from "react-icons/lu";
import type { UoocSnapshot, UoocTask } from "../state/types";
import { buildCourseTaskItems, type CourseTaskItem, type QuizState } from "../state/types";
import {
  checkinOngoing,
  courseTaskStatus,
  quizStageBusy,
  quizStageStatus,
  taskActive,
  videoStatus,
} from "../state/status";
import { usePending } from "../../use-pending";
import { CourseProgressView } from "./CourseProgressView";
import { QuizModePicker } from "../settings/AiSettings";
import { discussionNeeded, discussionStatusText } from "./discussion";

const seconds = (value: number) =>
  `${Math.floor(value / 60)}:${Math.floor(value % 60)
    .toString()
    .padStart(2, "0")}`;

export type CourseAction = (
  key: string,
  command: string,
  args?: Record<string, unknown>,
) => Promise<boolean>;

export interface CoursesTabProps {
  snapshot: UoocSnapshot;
  tasks: UoocTask[];
  ready: boolean;
  defaultMode: QuizState["mode"];
  action: CourseAction;
  openQuiz: (quiz: QuizState) => void;
  /** 打开页面级弹窗（删除单个任务 / 清理已完成）。 */
  requestDialog: (dialog: { kind: "deleteTask"; task: UoocTask } | { kind: "deleteCompleted" }) => void;
}

/** 课程 tab：一门课一张卡片，运行状态与主操作都在卡片上。 */
export function CoursesTab({
  snapshot,
  tasks,
  ready,
  defaultMode,
  action,
  openQuiz,
  requestDialog,
}: CoursesTabProps) {
  const pending = usePending();
  const taskByCourse = useMemo(() => {
    const map = new Map<string, UoocTask>();
    for (const task of tasks) map.set(task.course.id, task);
    return map;
  }, [tasks]);

  const runningCount = tasks.filter(taskActive).length;
  const pendingCheckins = snapshot.courses
    .filter(
      (course) =>
        snapshot.progress?.[course.id]?.data?.attendance === "needed" &&
        !checkinOngoing(snapshot.checkins?.[course.id]),
    )
    .map((course) => course.id);

  return (
    <div className="grid gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="text-sm text-muted">
          {runningCount > 0
            ? `${runningCount} 门课程运行中 · ${tasks.length} 门已加入`
            : `${snapshot.courses.length} 门开课中的课程`}
        </p>
        {(tasks.length > 0 || pendingCheckins.length > 0) && (
          <CoursesBatchMenu
            snapshot={snapshot}
            tasks={tasks}
            ready={ready}
            action={action}
            pendingCheckins={pendingCheckins}
            requestDialog={requestDialog}
          />
        )}
      </div>
      {pendingCheckins.length > 0 && (
        <Banner status="warning" className="flex flex-wrap items-center justify-between gap-3">
          <span className="grid gap-1">
            <Banner.Title>需要签到</Banner.Title>
            <Banner.Description>{pendingCheckins.length} 门课程需要签到</Banner.Description>
          </span>
          <Button
            variant="tonal"
            disabled={!ready || pending.isPending("checkin:bulk")}
            onClick={() =>
              void action("checkin:bulk", "start_uooc_checkin", { courseIds: pendingCheckins })
            }
          >
            {pending.isPending("checkin:bulk") && <LoadingIndicator size={16} />}
            签到全部 ({pendingCheckins.length})
          </Button>
        </Banner>
      )}
      {snapshot.courses.map((course) => (
        <CourseCard
          key={course.id}
          snapshot={snapshot}
          course={course}
          task={taskByCourse.get(course.id)}
          ready={ready}
          defaultMode={defaultMode}
          action={action}
          openQuiz={openQuiz}
          requestDialog={requestDialog}
          pending={pending}
        />
      ))}
      {!snapshot.courses.length && (
        <Card variant="outlined">
          <div className="p-4 text-muted">
            {ready ? "当前没有开课中的课程。" : "登录 UOOC 后查看开课中的课程。"}
          </div>
        </Card>
      )}
    </div>
  );
}

/** 批量操作菜单：每项按实际可操作的任务数启用。 */
function CoursesBatchMenu({
  snapshot,
  tasks,
  ready,
  action,
  pendingCheckins,
  requestDialog,
}: {
  snapshot: UoocSnapshot;
  tasks: UoocTask[];
  ready: boolean;
  action: CourseAction;
  pendingCheckins: string[];
  requestDialog: CoursesTabProps["requestDialog"];
}) {
  const [open, setOpen] = useState(false);
  const pausable = tasks.filter(taskActive);
  const resumable = tasks.filter((task) => !taskActive(task) && task.status !== "stopped");
  const stoppable = tasks.filter((task) => task.status !== "stopped");
  const completable = tasks.filter((task) => task.status === "completed");
  const startable = snapshot.courses
    .filter((course) => course.status !== "33" && !tasks.some((task) => task.course.id === course.id))
    .map((course) => course.id);
  const bulk = async (key: string, command: string, args: Record<string, unknown>) => {
    setOpen(false);
    await action(key, command, args);
  };
  return (
    <>
      <button
        type="button"
        id="uooc-courses-batch"
        className="top-app-bar-icon-button"
        aria-label="批量操作"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen(true)}
      >
        <LuEllipsisVertical aria-hidden="true" size={20} />
      </button>
      <ActionMenu
        anchorId="uooc-courses-batch"
        open={open}
        onOpenChange={setOpen}
        ariaLabel="批量操作"
        items={[
          {
            kind: "item",
            value: "start",
            label: `全部开始 (${startable.length})`,
            disabled: !ready || startable.length === 0,
          },
          {
            kind: "item",
            value: "pause",
            label: `暂停全部 (${pausable.length})`,
            disabled: !ready || pausable.length === 0,
          },
          {
            kind: "item",
            value: "resume",
            label: `继续全部 (${resumable.length})`,
            disabled: !ready || resumable.length === 0,
          },
          {
            kind: "item",
            value: "stop",
            label: `停止全部 (${stoppable.length})`,
            disabled: !ready || stoppable.length === 0,
          },
          { kind: "divider" },
          {
            kind: "item",
            value: "checkin",
            label: `签到全部待签到 (${pendingCheckins.length})`,
            disabled: !ready || pendingCheckins.length === 0,
          },
          {
            kind: "item",
            value: "clean",
            label: `清理已完成任务 (${completable.length})`,
            disabled: completable.length === 0,
          },
        ]}
        onSelect={(value) => {
          if (value === "start")
            void bulk("bulk:start", "start_uooc_courses", { courseIds: startable });
          if (value === "pause") void bulk("bulk:pause", "control_uooc_tasks", { courseIds: [], action: "pause" });
          if (value === "resume") void bulk("bulk:resume", "control_uooc_tasks", { courseIds: [], action: "resume" });
          if (value === "stop") void bulk("bulk:stop", "control_uooc_tasks", { courseIds: [], action: "stop" });
          if (value === "checkin")
            void bulk("checkin:bulk", "start_uooc_checkin", { courseIds: pendingCheckins });
          if (value === "clean") {
            setOpen(false);
            requestDialog({ kind: "deleteCompleted" });
          }
        }}
      />
    </>
  );
}

/** 一门课一张卡片：默认只露状态、进度与待办，其余进折叠区。 */
function CourseCard({
  snapshot,
  course,
  task,
  ready,
  defaultMode,
  action,
  openQuiz,
  requestDialog,
  pending,
}: {
  snapshot: UoocSnapshot;
  course: UoocSnapshot["courses"][number];
  task?: UoocTask;
  ready: boolean;
  defaultMode: QuizState["mode"];
  action: CourseAction;
  openQuiz: (quiz: QuizState) => void;
  requestDialog: CoursesTabProps["requestDialog"];
  pending: ReturnType<typeof usePending>;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [clock, setClock] = useState(() => Date.now());
  const isReview = course.status === "33";
  const progress = snapshot.progress?.[course.id]?.data;
  const videosCompleted = task?.videos.filter((v) => v.status === "completed").length ?? 0;
  const videosTotal = task?.videos.length ?? 0;
  const videosDone = videosTotal > 0 && videosCompleted === videosTotal;
  const status = isReview
    ? ({ label: "复习模式", tone: "warning" } as const)
    : task
      ? courseTaskStatus(task.status, videosDone)
      : ({ label: "未开始", tone: "default" } as const);
  const totalPercent = progress?.weightedPercent ?? progress?.totalPercent ?? null;
  const quiz = Object.values(snapshot.quizzes ?? {}).find(
    (q) => q.cid === course.id && (!snapshot.user || q.accountId === snapshot.user.id),
  );
  const needsDiscussion = discussionNeeded(snapshot, course.id);
  const discussionText = discussionStatusText(snapshot, course.id, clock);
  const needsCheckin =
    progress?.attendance === "needed" && !checkinOngoing(snapshot.checkins?.[course.id]);
  const quizNeedsAttention = quiz != null && !quizStageBusy(quiz.stage) && quiz.stage !== "passed" && quiz.stage !== "disabled";

  // 讨论状态含相对时间，仅在需要讨论时每秒刷新。
  useEffect(() => {
    if (!needsDiscussion) return;
    const timer = window.setInterval(() => setClock(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [needsDiscussion]);

  const taskItems = task ? buildCourseTaskItems(task.videos, quiz) : [];
  const pendingItems = taskItems.filter((item) => !itemCompleted(item));
  const completedItems = taskItems.filter(itemCompleted);
  const hasTestWeight = typeof progress?.test?.weight === "number" && progress.test.weight > 0;

  return (
    <Card id={`uooc-course-${course.id}`} variant="outlined">
      <div
        slot="headline"
        className="flex w-full flex-wrap items-center justify-between gap-2 font-medium text-lg"
      >
        <span>{course.title}</span>
        <span className="flex items-center gap-2">
          <Chip size="sm" color={status.tone}>
            {status.label}
          </Chip>
          {task && !isReview && (
            <CourseOverflowMenu
              open={menuOpen}
              onOpenChange={setMenuOpen}
              taskId={course.id}
              taskTitle={course.title}
              canStop={task.status !== "stopped"}
              onStop={() => void action(`task:${course.id}`, "control_uooc_tasks", { courseIds: [course.id], action: "stop" })}
              onDelete={() => requestDialog({ kind: "deleteTask", task })}
            />
          )}
        </span>
      </div>
      <div slot="subhead">
        {course.organization} · {course.semester}
        {task && videosTotal > 0 && (
          <span className="tabular-nums">
            {" "}
            · 视频 {videosCompleted}/{videosTotal}
          </span>
        )}
      </div>
      <div className="grid gap-3 p-4">
        {totalPercent != null && (
          <div className="flex items-center gap-3">
            <Progress
              variant="linear"
              aria-label={`${course.title}总进度`}
              value={Math.max(0, Math.min(100, totalPercent))}
              max={100}
            />
            <span className="shrink-0 text-xs text-muted tabular-nums">
              {Math.floor(totalPercent)}%
            </span>
          </div>
        )}
        {(needsCheckin || quizNeedsAttention || needsDiscussion) && (
          <div className="flex flex-wrap items-center gap-2">
            {needsCheckin && (
              <Button
                size="xs"
                variant="tonal"
                disabled={!ready || pending.isPending(`checkin:${course.id}`)}
                onClick={() =>
                  void action(`checkin:${course.id}`, "start_uooc_checkin", { courseIds: [course.id] })
                }
              >
                {pending.isPending(`checkin:${course.id}`) && <LoadingIndicator size={16} />}
                签到
              </Button>
            )}
            {quizNeedsAttention && (
              <Button size="xs" variant="tonal" onClick={() => openQuiz(quiz)}>
                <LuCircleHelp aria-hidden="true" />
                {quizStageStatus[quiz.stage].label}
              </Button>
            )}
            {needsDiscussion && (
              <Button
                size="xs"
                variant="tonal"
                disabled={
                  !ready ||
                  pending.isPending(`discussion:${course.id}`) ||
                  isReview
                }
                onClick={() =>
                  void action(`discussion:${course.id}`, "start_uooc_discussion", { courseId: course.id })
                }
              >
                {pending.isPending(`discussion:${course.id}`) ? (
                  <LoadingIndicator size={16} />
                ) : (
                  <LuMessageCircle aria-hidden="true" />
                )}
                参与讨论
              </Button>
            )}
          </div>
        )}
        {isReview && (
          <p className="text-xs text-muted">
            该课程处于复习模式（未结课），无法看视频或做测验/作业；考试请在「考核」标签中查看。
          </p>
        )}
        {task?.message && (
          <p role="status" className="text-sm text-muted">
            {task.message}
          </p>
        )}
        <Disclosure title="详情与操作" ariaLabel={`${course.title}的详情与操作`}>
          <div className="grid gap-4 pt-1">
            <div className="flex flex-wrap items-center gap-3">
              {task ? (
                <>
                  {taskActive(task) ? (
                    <Button
                      size="xs"
                      variant="tonal"
                      disabled={pending.isPending(`task:${course.id}`)}
                      onClick={() =>
                        void action(`task:${course.id}`, "control_uooc_tasks", {
                          courseIds: [course.id],
                          action: "pause",
                        })
                      }
                    >
                      <LuPause aria-hidden="true" />
                      暂停
                    </Button>
                  ) : (
                    <Button
                      size="xs"
                      variant="tonal"
                      disabled={!ready || pending.isPending(`task:${course.id}`)}
                      onClick={() =>
                        void action(`task:${course.id}`, "control_uooc_tasks", {
                          courseIds: [course.id],
                          action: "resume",
                        })
                      }
                    >
                      <LuPlay aria-hidden="true" />
                      继续
                    </Button>
                  )}
                </>
              ) : (
                !isReview && (
                  <Button
                    size="xs"
                    variant="tonal"
                    disabled={!ready || pending.isPending(`task:${course.id}`)}
                    onClick={async () => {
                      if (window.SzuApplicationBack) {
                        const { isPermissionGranted, requestPermission } =
                          await import("@tauri-apps/plugin-notification");
                        if (!(await isPermissionGranted())) await requestPermission();
                      }
                      await action(`task:${course.id}`, "start_uooc_courses", {
                        courseIds: [course.id],
                      });
                    }}
                  >
                    <LuPlay aria-hidden="true" />
                    开始刷课
                  </Button>
                )
              )}
              <div className="min-w-0 flex-1 basis-56">
                <QuizModePicker
                  value={quiz?.mode ?? defaultMode}
                  disabled={
                    isReview ||
                    !hasTestWeight ||
                    !ready ||
                    pending.isPending(`task:${course.id}`) ||
                    (task != null && taskActive(task))
                  }
                  description={
                    isReview
                      ? "复习模式下测验已截止"
                      : !hasTestWeight
                        ? "测验无权重占比，无需配置答题模式"
                        : undefined
                  }
                  onChange={(mode) =>
                    void action(`task:${course.id}`, "save_uooc_quiz_mode", { cid: course.id, mode })
                  }
                />
              </div>
            </div>
            <CheckinControls
              snapshot={snapshot}
              courseId={course.id}
              ready={ready}
              action={action}
              pending={pending}
            />
            {needsDiscussion && discussionText && (
              <p role="status" className="text-xs text-muted">
                {discussionText}
              </p>
            )}
            <CourseProgressView
              course={course}
              entry={snapshot.progress?.[course.id]}
              onRefresh={() =>
                void action(`progress:${course.id}`, "refresh_uooc_progress", {
                  courseIds: [course.id],
                })
              }
            />
            {taskItems.length > 0 && (
              <div className="grid gap-3">
                {completedItems.length > 0 && (
                  <Disclosure
                    title={
                      <span className="flex flex-1 items-center gap-2 text-sm font-medium">
                        <LuCheck className="shrink-0 text-success" aria-hidden="true" />
                        已完成 ({completedItems.length} 项)
                      </span>
                    }
                    ariaLabel={`已完成 (${completedItems.length} 项)`}
                  >
                    <div className="grid gap-3 pt-2">
                      {completedItems.map((item) => (
                        <TaskItemRow key={item.id} item={item} onOpenQuiz={openQuiz} />
                      ))}
                    </div>
                  </Disclosure>
                )}
                {pendingItems.map((item) => (
                  <TaskItemRow key={item.id} item={item} onOpenQuiz={openQuiz} />
                ))}
              </div>
            )}
          </div>
        </Disclosure>
      </div>
    </Card>
  );
}

function CourseOverflowMenu({
  open,
  onOpenChange,
  taskId,
  taskTitle,
  canStop,
  onStop,
  onDelete,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  taskId: string;
  taskTitle: string;
  canStop: boolean;
  onStop: () => void;
  onDelete: () => void;
}) {
  return (
    <>
      <button
        type="button"
        id={`uooc-task-menu-${taskId}`}
        className="top-app-bar-icon-button"
        aria-label={`${taskTitle}的任务操作`}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => onOpenChange(true)}
      >
        <LuEllipsisVertical aria-hidden="true" size={20} />
      </button>
      <ActionMenu
        anchorId={`uooc-task-menu-${taskId}`}
        open={open}
        onOpenChange={onOpenChange}
        ariaLabel="任务操作"
        items={[
          { kind: "item", value: "stop", label: "停止任务", disabled: !canStop },
          { kind: "item", value: "delete", label: "移除任务" },
        ]}
        onSelect={(value) => {
          onOpenChange(false);
          if (value === "stop") onStop();
          if (value === "delete") onDelete();
        }}
      />
    </>
  );
}

function CheckinControls({
  snapshot,
  courseId,
  ready,
  action,
  pending,
}: {
  snapshot: UoocSnapshot;
  courseId: string;
  ready: boolean;
  action: CourseAction;
  pending: ReturnType<typeof usePending>;
}) {
  const state = snapshot.checkins?.[courseId];
  const attendance = snapshot.progress?.[courseId]?.data?.attendance ?? "unknown";
  const message = state?.message;
  return (
    <div className="grid gap-2">
      <div className="flex flex-wrap items-center gap-2">
        {checkinOngoing(state) ? (
          <Button
            size="xs"
            variant="text"
            disabled={pending.isPending(`checkin:${courseId}`)}
            onClick={() =>
              void action(`checkin:${courseId}`, "cancel_uooc_checkin", { courseIds: [courseId] })
            }
          >
            取消签到
          </Button>
        ) : attendance === "unknown" && state?.status !== "failed" ? (
          <Button
            size="xs"
            variant="text"
            disabled={!ready || pending.isPending(`progress:${courseId}`)}
            onClick={() =>
              void action(`progress:${courseId}`, "refresh_uooc_progress", {
                courseIds: [courseId],
              })
            }
          >
            查询签到状态
          </Button>
        ) : null}
        {state?.status === "failed" && (
          <Button
            size="xs"
            variant="tonal"
            disabled={!ready || pending.isPending(`checkin:${courseId}`)}
            onClick={() =>
              void action(`checkin:${courseId}`, "start_uooc_checkin", { courseIds: [courseId] })
            }
          >
            手动重试签到
          </Button>
        )}
      </div>
      {message && (
        <p role="status" className="text-xs text-muted">
          {state?.source === "manual" ? "手动签到" : "自动签到"}：{message}
        </p>
      )}
    </div>
  );
}

function itemCompleted(item: CourseTaskItem): boolean {
  return item.kind === "video"
    ? item.video.status === "completed"
    : item.quiz.stage === "passed" || item.quiz.videoComplete;
}

function TaskItemRow({ item, onOpenQuiz }: { item: CourseTaskItem; onOpenQuiz: (quiz: QuizState) => void }) {
  if (item.kind === "quiz") {
    const status = quizStageStatus[item.quiz.stage];
    return (
      <button
        type="button"
        onClick={() => onOpenQuiz(item.quiz)}
        aria-label={`测验：${item.quiz.title || "课程测验"}，点击打开测验`}
        className="grid w-full gap-1.5 rounded-inner p-3 text-start transition-colors hover:bg-surface-secondary"
      >
        <span className="flex flex-wrap items-center justify-between gap-2 text-sm">
          <span className="flex items-center gap-2">
            <LuCircleHelp className="shrink-0 text-accent" aria-hidden="true" />
            <span className="font-medium">{item.quiz.title || "课程测验"}</span>
          </span>
          {status.tone === "accent" && quizStageBusy(item.quiz.stage) ? (
            <span className="flex items-center gap-1.5">
              <LoadingIndicator size={16} aria-label="处理中" />
              <span className="text-xs text-muted">{status.label}</span>
            </span>
          ) : (
            <Chip size="sm" color={status.tone}>
              {status.label}
            </Chip>
          )}
        </span>
        <span className="flex items-center justify-between text-xs text-muted">
          <span>{item.quiz.message || "点击打开弹窗查看和完成题目"}</span>
          <span className="text-accent">打开测验 ›</span>
        </span>
      </button>
    );
  }
  const video = item.video;
  const view = videoStatus[video.status];
  return (
    <div className="grid gap-1">
      <div className="flex flex-wrap items-center justify-between gap-2 text-sm">
        <span>{video.title}</span>
        <Chip size="sm" color={view.tone}>
          {view.label}
        </Chip>
      </div>
      <Progress
        variant="linear"
        aria-label={`${video.title}进度`}
        value={
          video.status === "completed"
            ? 100
            : video.duration
              ? Math.min(100, (video.position / video.duration) * 100)
              : 0
        }
        max={100}
        indeterminate={!video.duration && video.status === "running"}
      />
      <p className="text-xs text-muted tabular-nums">
        {seconds(video.position)} /{" "}
        {video.duration ? seconds(video.duration) : video.status === "completed" ? "时长未记录" : "等待时长"}
        {video.message && ` · ${video.message}`}
      </p>
    </div>
  );
}
