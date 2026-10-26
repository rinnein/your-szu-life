import { useEffect, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { Dialog, LoadingIndicator, Button } from "material/react";
import { createTaskChannel } from "../tauri-channel";
import { Banner, toast } from "../ui";
import { QuizPanel, type QuizReview } from "./QuizPanel";
import type {
  AiSessionEntry,
  ExamPaper,
  ExamTask,
  HomeworkTask,
  QuizAnswers,
  QuizQuestion,
  QuizState,
} from "./types";

interface ExamPaperModalProps {
  exam: ExamTask | HomeworkTask | null;
  isOpen: boolean;
  onClose: () => void;
  onSubmitted?: () => void;
  isHomework?: boolean;
}

export function ExamPaperModal({
  exam,
  isOpen,
  onClose,
  onSubmitted,
  isHomework = false,
}: ExamPaperModalProps) {
  const [loading, setLoading] = useState(false);
  const [paper, setPaper] = useState<ExamPaper | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [evalRequiredAlert, setEvalRequiredAlert] = useState<string | null>(null);

  const loadPaper = async () => {
    if (!exam) return;
    setLoading(true);
    setError(null);
    try {
      const res = await invoke<ExamPaper>("get_uooc_exam_paper", {
        cid: exam.cid,
        taskId: exam.taskId,
      });
      setPaper(res);
    } catch (err: unknown) {
      const msg = typeof err === "string" ? err : "无法加载试卷数据";
      setError(msg);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (!isOpen || !exam) {
      setPaper(null);
      setError(null);
      setEvalRequiredAlert(null);
      return;
    }
    void loadPaper();
  }, [isOpen, exam?.cid, exam?.taskId]);

  // 防作弊 30 秒心跳保持
  useEffect(() => {
    if (!isOpen || !exam || !paper || paper.isSubmitted) return;
    const timer = setInterval(() => {
      void invoke("heartbeat_uooc_exam", { cid: exam.cid, taskId: exam.taskId }).catch(() => {});
    }, 30000);
    return () => clearInterval(timer);
  }, [isOpen, exam?.cid, exam?.taskId, paper?.isSubmitted]);

  // 将 ExamPaper 转为 QuizState 适配 QuizPanel
  const quizState: QuizState | null = useMemo(() => {
    if (!paper || !exam) return null;
    return {
      accountId: exam.cid,
      cid: exam.cid,
      taskId: exam.taskId,
      resourceId: exam.taskId,
      title: paper.title || exam.title,
      mode: "supervised",
      stage: paper.isSubmitted ? "passed" : "awaitingApproval",
      revision: 1,
      questions: paper.questions,
      answers: paper.answers,
      reviews: paper.reviews,
      message: paper.isSubmitted ? "试卷已提交（仅供查看）" : "试卷加载完成",
      gated: false,
      videoComplete: true,
      studentAnswers: paper.answers,
      passed: paper.isSubmitted ? [exam.taskId] : [],
    };
  }, [paper, exam]);

  if (!isOpen || typeof document === "undefined") return null;

  return createPortal(
    <Dialog
      open={isOpen}
      headline={exam?.title || "考核试卷"}
      className="max-w-4xl"
      onClose={onClose}
    >
      <div className="flex flex-col max-h-[85vh] overflow-y-auto">
        {loading && (
          <div className="flex flex-col items-center justify-center p-12 space-y-4">
            <LoadingIndicator size={40} aria-label="正在加载试卷" />
            <p className="text-sm text-muted">正在加载试卷内容与题目...</p>
          </div>
        )}
        {!loading && error && (
          <div className="flex flex-col items-center justify-center p-12 space-y-4">
            <p className="text-sm text-danger font-medium">{error}</p>
            <Button variant="tonal" size="xs" onClick={() => void loadPaper()}>
              重试
            </Button>
          </div>
        )}
        {!loading && !error && quizState && exam && (
          <QuizPanel
            quiz={quizState}
            isExam={!isHomework}
            limitTime={paper?.limitTime || ("limitTime" in exam ? exam.limitTime : 0) || 0}
            warningAlert={
              evalRequiredAlert ? (
                <Banner status="warning" className="mb-2">
                  <Banner.Title>提示：需要先完成课程教学评价</Banner.Title>
                  <Banner.Description>{evalRequiredAlert}</Banner.Description>

                  <Button
                    size="xs"
                    variant="tonal"
                    onClick={() => void invoke("open_uooc_task_portal", { taskId: exam.taskId })}
                  >
                    前往官网完成评价
                  </Button>
                </Banner>
              ) : null
            }
            onMatchQa={async (questions: QuizQuestion[]) => {
              const res = await invoke<[QuizAnswers, QuizReview[]]>("match_uooc_paper_qa", {
                questions,
              });
              return { answers: res[0], reviews: res[1] };
            }}
            onSolveAi={async (
              questions: QuizQuestion[],
              onSessionEntry?: (entry: AiSessionEntry) => void,
            ) => {
              const channel = createTaskChannel<AiSessionEntry>((entry) => {
                onSessionEntry?.(entry);
              });
              const res = await invoke<[QuizAnswers, QuizReview[]]>("solve_uooc_paper_ai", {
                questions,
                onEvent: channel,
              });
              return { answers: res[0], reviews: res[1] };
            }}
            onSaveDraft={async (answers: QuizAnswers) => {
              await invoke("save_uooc_exam_draft", {
                cid: exam.cid,
                taskId: exam.taskId,
                answers,
              });
            }}
            onSubmit={async (answers: QuizAnswers) => {
              try {
                await invoke("submit_uooc_exam", {
                  cid: exam.cid,
                  taskId: exam.taskId,
                  answers,
                });
                toast.success("试卷已成功提交");
                onSubmitted?.();
              } catch (err: unknown) {
                const msg =
                  typeof err === "string"
                    ? err
                    : (err as { message?: string })?.message || "提交失败";
                if (msg.includes("课程评价") || msg.includes("601")) {
                  setEvalRequiredAlert(msg);
                }
                throw err;
              }
            }}
            onOpenPortal={async () => {
              await invoke("open_uooc_task_portal", { taskId: exam.taskId });
            }}
            onSyncPortal={async () => {
              await loadPaper();
            }}
            onGenerateSubjective={async (question: QuizQuestion) => {
              return await invoke<string>("generate_uooc_subjective", {
                stem: question.text,
                question,
                context: null,
                referenceExamples: null,
              });
            }}
            onClose={onClose}
          />
        )}
      </div>
    </Dialog>,
    document.body,
  );
}
