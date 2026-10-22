import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Alert, Button, Card, Chip, Label, ListBox, Select, Spinner, toast } from "@heroui/react";
import { LuBookOpen, LuCalendar, LuCircleCheck, LuRotateCw } from "react-icons/lu";
import { ExamPaperModal } from "./ExamPaperModal";
import type { HomeworkTask, QuizState, UoocCourse, UoocSnapshot } from "./types";

interface QuizListPanelProps {
  courses: UoocCourse[];
  quizzes?: Record<string, QuizState>;
  onOpenQuiz?: (quiz: QuizState) => void;
  onSnapshot?: (snapshot: UoocSnapshot) => void;
  ready: boolean;
}

export function QuizListPanel({
  courses,
  quizzes,
  onOpenQuiz,
  onSnapshot,
  ready,
}: QuizListPanelProps) {
  const [quizTasks, setQuizTasks] = useState<HomeworkTask[]>([]);
  const [loading, setLoading] = useState(false);
  const [selectedCid, setSelectedCid] = useState<string>("all");
  const [activeQuiz, setActiveQuiz] = useState<HomeworkTask | null>(null);

  const fetchQuizzes = async (cidOption?: string) => {
    if (!ready) return;
    setLoading(true);
    const targetCid = cidOption === "all" ? undefined : cidOption || undefined;
    try {
      const res = await invoke<HomeworkTask[]>("list_uooc_quizzes", {
        cid: targetCid || null,
      });
      setQuizTasks(res || []);
    } catch (err: unknown) {
      const msg = typeof err === "string" ? err : "拉取测验列表失败";
      toast.danger(msg);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (ready) {
      void fetchQuizzes(selectedCid);
    }
  }, [ready, selectedCid]);

  const getQuizDisplayStatus = (q: HomeworkTask) => {
    // 测验的状态判断不依赖 end_time，依据 state.do, state.view, statusText, statusCode 与 score 判定
    const isCompleted =
      q.canView ||
      (q.statusCode !== undefined && q.statusCode >= 20) ||
      q.statusText === "已批改" ||
      (q.score !== null && q.score !== undefined);

    if (isCompleted) {
      return {
        label:
          q.score !== null && q.score !== undefined
            ? `已批改: ${q.score}分`
            : q.statusText || "已通过",
        color: "success" as const,
        actionLabel: "查看答卷",
        canEnter: true,
      };
    }

    if (q.statusText === "待批改" || q.statusCode === 10) {
      return {
        label: "待批改",
        color: "warning" as const,
        actionLabel: "查看详情",
        canEnter: true,
      };
    }

    if (q.canDo) {
      const inProgress = !!q.doTime && !q.commitTime;
      return {
        label: inProgress ? "作答中" : q.statusText || "未提交",
        color: "accent" as const,
        actionLabel: "进入测验",
        canEnter: true,
      };
    }

    return {
      label: q.statusText || "未开放",
      color: "default" as const,
      actionLabel: "未开放",
      canEnter: false,
    };
  };

  const handleQuizAction = async (task: HomeworkTask) => {
    const isCompleted =
      task.canView ||
      (task.statusCode !== undefined && task.statusCode >= 20) ||
      task.statusText === "已批改" ||
      (task.score !== null && task.score !== undefined);

    if (isCompleted) {
      setLoading(true);
      try {
        const nextSnapshot = await invoke<UoocSnapshot>("fetch_uooc_completed_quiz", {
          cid: task.cid,
          taskId: task.taskId || task.id,
        });
        if (nextSnapshot) {
          onSnapshot?.(nextSnapshot);
          const loadedQuiz = Object.values(nextSnapshot.quizzes ?? {}).find(
            (q) => q.cid === task.cid && (q.taskId === task.taskId || q.taskId === task.id),
          );
          if (loadedQuiz && onOpenQuiz) {
            onOpenQuiz(loadedQuiz);
            return;
          }
        }
      } catch {
        // 静默回退到 ExamPaperModal
      } finally {
        setLoading(false);
      }
    }

    const liveQuiz = Object.values(quizzes ?? {}).find(
      (qz) => qz.cid === task.cid && (qz.taskId === task.taskId || qz.taskId === task.id),
    );
    if (liveQuiz && onOpenQuiz) {
      onOpenQuiz(liveQuiz);
      return;
    }
    setActiveQuiz(task);
  };

  return (
    <div className="grid gap-6 pt-4">
      {/* 顶部控制栏 */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
        <div className="flex flex-wrap items-center gap-2.5 w-full sm:w-auto">
          <Select
            aria-label="选择课程筛选"
            className="w-full sm:w-64"
            value={selectedCid}
            onChange={(val) => {
              if (typeof val === "string") setSelectedCid(val);
            }}
          >
            <Label className="sr-only">课程筛选</Label>
            <Select.Trigger>
              <Select.Value />
              <Select.Indicator />
            </Select.Trigger>
            <Select.Popover>
              <ListBox>
                <ListBox.Item id="all" textValue="全部课程">
                  全部课程
                  <ListBox.ItemIndicator />
                </ListBox.Item>
                {courses.map((c) => (
                  <ListBox.Item
                    key={c.id}
                    id={c.id}
                    textValue={c.status === "33" ? `${c.title} (复习中)` : c.title}
                  >
                    <div className="flex items-center justify-between w-full">
                      <span>{c.title}</span>
                      {c.status === "33" && (
                        <span className="text-xs text-warning ml-2 font-normal">复习中</span>
                      )}
                    </div>
                    <ListBox.ItemIndicator />
                  </ListBox.Item>
                ))}
              </ListBox>
            </Select.Popover>
          </Select>

          <Button
            variant="secondary"
            isDisabled={loading || !ready}
            onPress={() => void fetchQuizzes(selectedCid)}
          >
            {loading ? <Spinner size="sm" /> : <LuRotateCw />}
            刷新列表
          </Button>
        </div>

        <p className="text-sm text-muted">
          共收录 {quizTasks.length} 项章节测验任务 · 支持客观题题库单题匹配与 AI 判定
        </p>
      </div>

      {/* 提示信息 */}
      <Alert status="accent" className="flex items-center gap-2 text-xs">
        <Alert.Indicator />
        <Alert.Content>
          <Alert.Title>测验说明</Alert.Title>
          <Alert.Description>
            随堂与章节测验支持单题题库匹配与高相似度向量检索。答题时支持草稿暂存与实时判定，可在列表中直接进入作答并确认提交。
          </Alert.Description>
        </Alert.Content>
      </Alert>

      {/* 列表内容 */}
      {loading && quizTasks.length === 0 ? (
        <div className="flex h-48 flex-col items-center justify-center gap-3">
          <Spinner size="lg" />
          <p className="text-sm text-muted">正在检索课程测验任务...</p>
        </div>
      ) : quizTasks.length === 0 ? (
        <Card className="p-8 text-center">
          <p className="text-muted">当前课程下暂无开放或收录的章节测验任务</p>
        </Card>
      ) : (
        <div className="grid gap-4 md:grid-cols-2">
          {quizTasks.map((task) => {
            const statusInfo = getQuizDisplayStatus(task);

            return (
              <Card key={task.id} className="flex flex-col justify-between border border-border/60">
                <Card.Header className="flex flex-wrap items-start justify-between gap-2 pb-2">
                  <div className="space-y-1">
                    {(task.chapterText || task.sectionText) && (
                      <p className="text-[11px] font-medium text-accent">
                        {[task.chapterText, task.sectionText].filter(Boolean).join(" · ")}
                      </p>
                    )}
                    <Card.Title className="text-base font-bold leading-snug">
                      {task.title}
                    </Card.Title>
                    <p className="flex items-center gap-1 text-xs text-muted">
                      <LuBookOpen className="h-3 w-3" />
                      {task.courseName}
                    </p>
                  </div>

                  <Chip variant="soft" color={statusInfo.color} size="sm">
                    {statusInfo.label}
                  </Chip>
                </Card.Header>

                <Card.Content className="space-y-2 py-2 text-xs text-muted">
                  <div className="flex items-center gap-2">
                    <LuCalendar className="h-3.5 w-3.5" />
                    <span>
                      {task.commitTime
                        ? `提交时间: ${task.commitTime}`
                        : task.doTime
                          ? `作答时间: ${task.doTime}`
                          : `起止时间: ${task.startTime || "常开"} ~ ${task.endTime || "课期结束"}`}
                    </span>
                  </div>

                  <div className="flex flex-wrap items-center gap-4">
                    <span className="flex items-center gap-1">
                      <LuCircleCheck className="h-3.5 w-3.5" />
                      作答次数:{" "}
                      {task.chance > 0
                        ? `已答 ${task.submitCount} / 共 ${task.chance} 次`
                        : "不限次数"}
                    </span>
                  </div>
                </Card.Content>

                <Card.Footer className="flex items-center justify-between border-t bg-surface/30 pt-3">
                  <span className="font-mono text-xs text-muted">ID: {task.taskId}</span>

                  <Button
                    variant="primary"
                    size="sm"
                    isDisabled={!statusInfo.canEnter}
                    onPress={() => handleQuizAction(task)}
                  >
                    {statusInfo.actionLabel}
                  </Button>
                </Card.Footer>
              </Card>
            );
          })}
        </div>
      )}

      {/* 测验答题模态框 */}
      {activeQuiz && (
        <ExamPaperModal
          exam={activeQuiz}
          isOpen={!!activeQuiz}
          onClose={() => setActiveQuiz(null)}
          onSubmitted={() => void fetchQuizzes(selectedCid)}
          isHomework={false}
        />
      )}
    </div>
  );
}
