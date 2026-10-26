import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, Divider, LoadingIndicator, Select, SelectOption, Button } from "material/react";
import { Banner, Chip, toast } from "../ui";
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
            label="课程筛选"
            aria-label="选择课程筛选"
            className="w-full sm:w-64"
            value={selectedCid}
            onChange={(e: any) => {
              const val = e.detail?.value ?? e.target?.value;
              if (typeof val === "string") setSelectedCid(val);
            }}
          >
            <SelectOption value="all">全部课程</SelectOption>
            {courses.map((c) => (
              <SelectOption key={c.id} value={c.id}>
                {c.status === "33" ? `${c.title} (复习中)` : c.title}
              </SelectOption>
            ))}
          </Select>

          <Button
            variant="tonal"
            disabled={loading || !ready}
            onClick={() => void fetchQuizzes(selectedCid)}
          >
            {loading ? <LoadingIndicator size={16} /> : <LuRotateCw />}
            刷新列表
          </Button>
        </div>

        <p className="text-sm text-muted">
          共收录 {quizTasks.length} 项章节测验任务 · 支持客观题题库单题匹配与 AI 判定
        </p>
      </div>

      {/* 提示信息 */}
      <Banner status="accent" className="flex items-center gap-2 text-xs">
        <Banner.Title>测验说明</Banner.Title>
        <Banner.Description>
          随堂与章节测验支持单题题库匹配与高相似度向量检索。答题时支持草稿暂存与实时判定，可在列表中直接进入作答并确认提交。
        </Banner.Description>
      </Banner>

      {/* 列表内容 */}
      {loading && quizTasks.length === 0 ? (
        <div className="flex h-48 flex-col items-center justify-center gap-3">
          <LoadingIndicator size={40} />
          <p className="text-sm text-muted">正在检索课程测验任务...</p>
        </div>
      ) : quizTasks.length === 0 ? (
        <Card variant="outlined" className="p-8 text-center">
          <p className="text-muted">当前课程下暂无开放或收录的章节测验任务</p>
        </Card>
      ) : (
        <div className="grid gap-4 md:grid-cols-2">
          {quizTasks.map((task) => {
            const statusInfo = getQuizDisplayStatus(task);

            return (
              <Card key={task.id} variant="outlined" className="flex flex-col justify-between">
                <div className="flex flex-wrap items-start justify-between gap-2 p-4 pb-2">
                  <div className="space-y-1">
                    {(task.chapterText || task.sectionText) && (
                      <p className="text-[11px] font-medium text-accent">
                        {[task.chapterText, task.sectionText].filter(Boolean).join(" · ")}
                      </p>
                    )}
                    <h3 className="text-base font-bold leading-snug">{task.title}</h3>
                    <p className="flex items-center gap-1 text-xs text-muted">
                      <LuBookOpen className="h-3 w-3" />
                      {task.courseName}
                    </p>
                  </div>

                  <Chip variant="soft" color={statusInfo.color} size="sm">
                    {statusInfo.label}
                  </Chip>
                </div>

                <div className="space-y-2 p-4 py-2 text-xs text-muted">
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
                </div>

                <Divider />
                <div className="flex items-center justify-between bg-surface/30 p-4 pt-3">
                  <span className="font-mono text-xs text-muted">ID: {task.taskId}</span>

                  <Button
                    variant="filled"
                    size="xs"
                    disabled={!statusInfo.canEnter}
                    onClick={() => handleQuizAction(task)}
                  >
                    {statusInfo.actionLabel}
                  </Button>
                </div>
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
