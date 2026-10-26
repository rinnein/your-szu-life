import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { listen } from "@tauri-apps/api/event";
import { Card, Dialog, LoadingIndicator, Progress, Button } from "material/react";
import { Chip, ScrollShadow } from "../ui";
import { LuBookOpen, LuCheck, LuCircleCheck, LuCircleHelp, LuFileText } from "react-icons/lu";
import { DimmedHtmlText } from "./DimmedHtmlText";

export type BankRebuildProgress = {
  stage: "start" | "course" | "task" | "question" | "task_complete" | "complete" | "error";
  cid: string;
  courseName: string;
  tid?: string;
  taskName?: string;
  questionId?: string;
  questionStem?: string;
  current: number;
  total: number;
  message: string;
};

type RebuildQuestionItem = {
  id: string;
  stem: string;
};

type RebuildTaskItem = {
  tid: string;
  taskName: string;
  status: "processing" | "completed";
  questions: RebuildQuestionItem[];
};

type RebuildCourseItem = {
  cid: string;
  courseName: string;
  status: "scanning" | "completed";
  tasks: Record<string, RebuildTaskItem>;
};

export function BankRebuildModal({
  isOpen,
  onOpenChange,
  isBuilding,
  onComplete,
}: {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  isBuilding: boolean;
  onComplete?: () => void;
}) {
  const [coursesMap, setCoursesMap] = useState<Record<string, RebuildCourseItem>>({});
  const [latestMessage, setLatestMessage] = useState<string>("正在初始化构建任务...");
  const [activeCourseId, setActiveCourseId] = useState<string>("");
  const [activeTaskId, setActiveTaskId] = useState<string>("");
  const [isCompleted, setIsCompleted] = useState(false);
  const [totalQuestionsCollected, setTotalQuestionsCollected] = useState(0);

  // 当打开 Modal 时若重置构建，重置内部状态
  useEffect(() => {
    if (isOpen && isBuilding) {
      setCoursesMap({});
      setLatestMessage("开始扫描课程与题库...");
      setActiveCourseId("");
      setActiveTaskId("");
      setIsCompleted(false);
      setTotalQuestionsCollected(0);
    }
  }, [isOpen, isBuilding]);

  useEffect(() => {
    let active = true;
    let stop: (() => void) | undefined;

    void listen<BankRebuildProgress>("uooc-bank-rebuild-progress", (event) => {
      if (!active) return;
      const p = event.payload;
      setLatestMessage(p.message);

      if (p.stage === "complete") {
        setIsCompleted(true);
        setTotalQuestionsCollected(p.total);
        setCoursesMap((prev) => {
          const next = { ...prev };
          for (const c of Object.values(next)) {
            c.status = "completed";
            for (const t of Object.values(c.tasks)) {
              t.status = "completed";
            }
          }
          return next;
        });
        return;
      }

      if (p.cid) {
        setActiveCourseId(p.cid);
        setCoursesMap((prev) => {
          const next = { ...prev };
          const course = next[p.cid] ?? {
            cid: p.cid,
            courseName: p.courseName || `课程 ${p.cid}`,
            status: "scanning",
            tasks: {},
          };

          if (p.tid) {
            setActiveTaskId(p.tid);
            const task = course.tasks[p.tid] ?? {
              tid: p.tid,
              taskName: p.taskName || `测验 ${p.tid}`,
              status: "processing",
              questions: [],
            };

            if (p.questionId) {
              if (!task.questions.some((q) => q.id === p.questionId)) {
                task.questions.push({
                  id: p.questionId,
                  stem: p.questionStem || `题目 ${p.questionId}`,
                });
              }
            }

            if (p.stage === "task_complete") {
              task.status = "completed";
            }

            course.tasks[p.tid] = task;
          }

          next[p.cid] = course;
          return next;
        });
      }
    }).then((unlisten) => {
      if (!active) unlisten();
      else stop = unlisten;
    });

    return () => {
      active = false;
      stop?.();
    };
  }, []);

  const courseList = Object.values(coursesMap);

  if (!isOpen || typeof document === "undefined") return null;

  return createPortal(
    <Dialog
      open={isOpen}
      headline="题库构建流程进度"
      className="max-w-4xl"
      onClose={() => {
        if (!isBuilding || isCompleted) {
          onOpenChange(false);
        }
      }}
    >
      <div className="flex flex-col gap-4 max-h-[70vh] overflow-hidden">
        <div className="flex flex-wrap items-center justify-between gap-3 w-full pr-2">
          <div>
            <p className="text-xs text-muted mt-1">
              实时层级追踪：cid (课程) → tid (测验题单) → 题目 ID (真实全局题目)
            </p>
          </div>
          {isCompleted ? (
            <Chip color="success" size="sm">
              <LuCheck aria-hidden="true" />
              构建完成
            </Chip>
          ) : (
            <div className="flex items-center gap-2">
              <LoadingIndicator size={16} aria-label="构建中" />
              <Chip color="warning" size="sm">
                正在解析收录
              </Chip>
            </div>
          )}
        </div>

        <div className="grid gap-4 overflow-hidden">
          <div className="rounded-xl border border-separator/60 bg-surface/40 p-3.5 text-sm">
            <div className="flex items-center justify-between gap-2 mb-2">
              <span className="font-semibold text-xs text-muted uppercase tracking-wider">
                当前状态
              </span>
              {isCompleted && (
                <span className="text-xs font-medium text-success flex items-center gap-1">
                  <LuCircleCheck aria-hidden="true" />
                  共收录 {totalQuestionsCollected} 道题目
                </span>
              )}
            </div>
            <p className="text-foreground font-mono text-sm break-all">{latestMessage}</p>
            {!isCompleted && isBuilding && (
              <Progress variant="linear" indeterminate aria-label="构建进度" className="mt-2.5" />
            )}
          </div>

          {/* 层级流程展示：cid (+课程名) -> tid (/task_id +测验名) -> 题目id */}
          <ScrollShadow className="grid gap-3.5 pr-1 max-h-[46vh] overflow-y-auto">
            {courseList.length === 0 ? (
              <Card variant="outlined">
                <div className="p-4 py-8 text-center text-sm text-muted">
                  <LoadingIndicator size={24} className="mx-auto mb-2" />
                  正在扫描参与课程的完整目录与测验信息...
                </div>
              </Card>
            ) : (
              courseList.map((course) => {
                const taskList = Object.values(course.tasks);
                const isCourseActive = activeCourseId === course.cid && !isCompleted;
                return (
                  <Card
                    key={course.cid}
                    variant="outlined"
                    className={
                      isCourseActive
                        ? "border-accent/60 bg-accent/5 shadow-sm"
                        : "border-separator/50"
                    }
                  >
                    <div className="p-3 pb-2">
                      <div className="flex flex-wrap items-center justify-between gap-2 w-full">
                        <div className="flex items-center gap-2">
                          <LuBookOpen className="text-primary shrink-0" aria-hidden="true" />
                          <h3 className="text-sm font-semibold">{course.courseName}</h3>
                          <span className="font-mono text-xs text-muted">(cid: {course.cid})</span>
                        </div>
                        <Chip
                          size="sm"
                          color={course.status === "completed" ? "success" : "accent"}
                        >
                          {course.status === "completed" ? "已完成" : "扫描中"}
                        </Chip>
                      </div>
                    </div>
                    <div className="grid gap-2.5 p-3 pt-0">
                      {taskList.length === 0 ? (
                        <p className="text-xs text-muted pl-6 py-1">
                          正在解析该课程章节目录与测验题组...
                        </p>
                      ) : (
                        taskList.map((task) => {
                          const isTaskActive = activeTaskId === task.tid && !isCompleted;
                          return (
                            <div
                              key={task.tid}
                              className={`rounded-lg border p-2.5 transition-colors ${
                                isTaskActive
                                  ? "border-accent bg-surface/90 shadow-sm"
                                  : "border-separator/40 bg-surface/30"
                              }`}
                            >
                              <div className="flex flex-wrap items-center justify-between gap-2 mb-2">
                                <div className="flex items-center gap-2">
                                  <LuFileText
                                    className="text-accent shrink-0 text-sm"
                                    aria-hidden="true"
                                  />
                                  <span className="text-xs font-semibold text-foreground">
                                    {task.taskName}
                                  </span>
                                  <span className="font-mono text-[11px] text-muted">
                                    (tid: {task.tid})
                                  </span>
                                </div>
                                <div className="flex items-center gap-1.5">
                                  <Chip size="sm" variant="secondary">
                                    {task.questions.length} 题
                                  </Chip>
                                  {task.status === "completed" ? (
                                    <Chip size="sm" color="success">
                                      已就绪
                                    </Chip>
                                  ) : (
                                    <Chip size="sm" color="warning">
                                      解析中
                                    </Chip>
                                  )}
                                </div>
                              </div>

                              {/* 题目 ID 流程组展示 */}
                              {task.questions.length > 0 && (
                                <div className="grid gap-1 pl-4 border-l-2 border-separator/60 mt-1.5">
                                  {task.questions.map((q) => (
                                    <div
                                      key={q.id}
                                      className="flex items-center justify-between gap-2 py-0.5 text-xs"
                                    >
                                      <div className="flex items-center gap-1.5 min-w-0">
                                        <LuCircleHelp
                                          className="text-muted shrink-0 text-[11px]"
                                          aria-hidden="true"
                                        />
                                        <span className="font-mono font-medium text-[11px] text-accent">
                                          ID: {q.id}
                                        </span>
                                        <span className="truncate text-muted text-[11px]">
                                          <DimmedHtmlText text={q.stem} />
                                        </span>
                                      </div>
                                      <span className="text-[10px] text-success shrink-0 font-medium">
                                        ✓ 已收录
                                      </span>
                                    </div>
                                  ))}
                                </div>
                              )}
                            </div>
                          );
                        })
                      )}
                    </div>
                  </Card>
                );
              })
            )}
          </ScrollShadow>
        </div>
      </div>

      <div slot="actions">
        <Button
          variant={isCompleted ? "filled" : "tonal"}
          disabled={!isCompleted && isBuilding}
          onClick={() => {
            onOpenChange(false);
            onComplete?.();
          }}
        >
          {!isCompleted && isBuilding ? (
            <>
              <LoadingIndicator size={16} />
              正在构建题库...
            </>
          ) : (
            "完成并查看题库"
          )}
        </Button>
      </div>
    </Dialog>,
    document.body,
  );
}
