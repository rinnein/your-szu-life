import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Button, Card, Chip, Modal, ProgressBar, ScrollShadow, Spinner } from "@heroui/react";
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

  return (
    <Modal
      isOpen={isOpen}
      onOpenChange={(open) => {
        if (!isBuilding || isCompleted) {
          onOpenChange(open);
        }
      }}
    >
      <Modal.Backdrop>
        <Modal.Container size="cover" className="max-w-4xl">
          <Modal.Dialog aria-label="本地题库构建流程进度">
            <Modal.Header>
              <div className="flex flex-wrap items-center justify-between gap-3 w-full pr-6">
                <div>
                  <Modal.Heading>本地题库构建流程进度</Modal.Heading>
                  <p className="text-xs text-muted mt-1">
                    实时层级追踪：cid (课程) → tid (测验题单) → 题目 ID (真实全局题目)
                  </p>
                </div>
                {isCompleted ? (
                  <Chip color="success" size="sm">
                    <LuCheck aria-hidden="true" />
                    <Chip.Label>构建完成</Chip.Label>
                  </Chip>
                ) : (
                  <div className="flex items-center gap-2">
                    <Spinner size="sm" aria-label="构建中" />
                    <Chip color="warning" size="sm">
                      <Chip.Label>正在解析收录</Chip.Label>
                    </Chip>
                  </div>
                )}
              </div>
            </Modal.Header>
            <Modal.Body className="grid gap-4 max-h-[70vh] overflow-hidden">
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
                  <ProgressBar isIndeterminate aria-label="构建进度" className="mt-2.5" />
                )}
              </div>

              {/* 层级流程展示：cid (+课程名) -> tid (/task_id +测验名) -> 题目id */}
              <ScrollShadow className="grid gap-3.5 pr-1 max-h-[46vh] overflow-y-auto">
                {courseList.length === 0 ? (
                  <Card>
                    <Card.Content className="py-8 text-center text-sm text-muted">
                      <Spinner size="md" className="mx-auto mb-2" />
                      正在扫描参与课程的完整目录与测验信息...
                    </Card.Content>
                  </Card>
                ) : (
                  courseList.map((course) => {
                    const taskList = Object.values(course.tasks);
                    const isCourseActive = activeCourseId === course.cid && !isCompleted;
                    return (
                      <Card
                        key={course.cid}
                        className={
                          isCourseActive
                            ? "border-accent/60 bg-accent/5 shadow-sm"
                            : "border-separator/50"
                        }
                      >
                        <Card.Header className="pb-2">
                          <div className="flex flex-wrap items-center justify-between gap-2 w-full">
                            <div className="flex items-center gap-2">
                              <LuBookOpen className="text-primary shrink-0" aria-hidden="true" />
                              <Card.Title className="text-sm font-semibold">
                                {course.courseName}
                              </Card.Title>
                              <span className="font-mono text-xs text-muted">
                                (cid: {course.cid})
                              </span>
                            </div>
                            <Chip
                              size="sm"
                              color={course.status === "completed" ? "success" : "accent"}
                            >
                              <Chip.Label>
                                {course.status === "completed" ? "已完成" : "扫描中"}
                              </Chip.Label>
                            </Chip>
                          </div>
                        </Card.Header>
                        <Card.Content className="grid gap-2.5 pt-0">
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
                                        <Chip.Label>{task.questions.length} 题</Chip.Label>
                                      </Chip>
                                      {task.status === "completed" ? (
                                        <Chip size="sm" color="success">
                                          <Chip.Label>已就绪</Chip.Label>
                                        </Chip>
                                      ) : (
                                        <Chip size="sm" color="warning">
                                          <Chip.Label>解析中</Chip.Label>
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
                        </Card.Content>
                      </Card>
                    );
                  })
                )}
              </ScrollShadow>
            </Modal.Body>
            <Modal.Footer>
              <Button
                variant={isCompleted ? "primary" : "secondary"}
                isDisabled={!isCompleted && isBuilding}
                onPress={() => {
                  onOpenChange(false);
                  onComplete?.();
                }}
              >
                {!isCompleted && isBuilding ? (
                  <>
                    <Spinner size="sm" />
                    正在构建题库...
                  </>
                ) : (
                  "完成并查看题库"
                )}
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}
