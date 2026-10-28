import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  ButtonGroup,
  Checkbox,
  Dialog,
  LoadingIndicator,
  Progress,
  Radio,
  Radios,
  Switch,
  TextField,
  Button,
} from "material/react";
import { Banner, Chip, toast } from "../ui";
import { LuBot, LuCircleCheck, LuClock, LuDatabase, LuSparkles } from "react-icons/lu";
import { DimmedHtmlText, useHtmlTagVisibility } from "./DimmedHtmlText";
import type { AiSessionEntry, QuizAnswers, QuizQuestion, QuizState, UoocSnapshot } from "./types";

export function getQuizCacheKey(
  accountId?: string,
  cid?: string,
  taskId?: string,
  resourceId?: string,
): string {
  return `uooc_quiz_cache_${accountId || "anon"}_${cid || "unknown"}_${taskId || resourceId || "default"}`;
}

export function loadQuizCache(key: string): QuizAnswers | null {
  try {
    if (typeof localStorage === "undefined") return null;
    const raw = localStorage.getItem(key);
    if (!raw) return null;
    return JSON.parse(raw) as QuizAnswers;
  } catch {
    return null;
  }
}

export function saveQuizCache(key: string, answers: QuizAnswers): void {
  try {
    if (typeof localStorage === "undefined") return;
    localStorage.setItem(key, JSON.stringify(answers));
  } catch {
    // Ignore quota or private mode errors
  }
}

export function clearQuizCache(key: string): void {
  try {
    if (typeof localStorage === "undefined") return;
    localStorage.removeItem(key);
  } catch {
    // Ignore
  }
}

export const HTML_TAG_REGEX = /<(?:\/?[a-zA-Z][a-zA-Z0-9:-]*(?:\s+[^>]*?)?\/?|!--[\s\S]*?--)>/gi;

export function stripHtml(raw?: string | null): string {
  if (!raw) return "";
  return raw
    .replace(HTML_TAG_REGEX, " ")
    .replace(/&(?:nbsp|ensp|emsp|thinsp);/gi, " ")
    .replace(/&lt;/gi, "<")
    .replace(/&gt;/gi, ">")
    .replace(/&amp;/gi, "&")
    .replace(/&quot;/gi, '"')
    .replace(/&(?:#39|apos);/gi, "'")
    .replace(/&#(\d+);/g, (_, dec) => String.fromCharCode(Number(dec)))
    .replace(/&#x([0-9a-fA-F]+);/gi, (_, hex) => String.fromCharCode(parseInt(hex, 16)))
    .split(/\s+/)
    .join(" ")
    .trim();
}

export function isOptionMatch(opt: { id: string; text: string }, target?: string | null): boolean {
  if (!target) return false;
  const t = target.trim();
  if (!t) return false;

  const tLower = t.toLowerCase();
  const oId = opt.id.trim();
  const oIdLower = oId.toLowerCase();

  // 1. 直接 ID 比对 (如 "A" === "a", "opt_1" === "opt_1")
  if (oIdLower === tLower) return true;

  // 1.1 括号前 ID 比对，如 "B (1983年)" -> "B"
  const beforeParen = t.split(/[(（]/)[0]?.trim();
  if (beforeParen && beforeParen.toLowerCase() === oIdLower) return true;

  // 2. 原始文本匹配
  const oTextTrim = opt.text.trim();
  if (oTextTrim === t || oTextTrim.toLowerCase() === tLower) return true;

  // 3. 去除 HTML 标签后纯文本匹配
  const cleanTarget = stripHtml(t);
  const cleanOpt = stripHtml(opt.text);
  if (cleanOpt.length > 0 && cleanTarget.length > 0) {
    if (cleanOpt === cleanTarget || cleanOpt.toLowerCase() === cleanTarget.toLowerCase()) {
      return true;
    }
  }

  // 4. 剥离前缀序号匹配 (如 "A. 选项内容" vs "选项内容")
  const stripPrefix = (s: string) => s.replace(/^[a-zA-Z0-9][.、:：\s]+/, "").trim();
  const strippedOpt = stripPrefix(cleanOpt);
  const strippedTarget = stripPrefix(cleanTarget);
  if (strippedOpt.length > 0) {
    if (
      strippedOpt === cleanTarget ||
      strippedOpt === strippedTarget ||
      strippedOpt.toLowerCase() === cleanTarget.toLowerCase() ||
      strippedOpt.toLowerCase() === strippedTarget.toLowerCase()
    ) {
      return true;
    }
  }

  // 4.1 选项文本去括号后比对，如 "1983年" vs "1983年 (建校)"
  const cleanTargetBeforeParen = stripHtml(beforeParen);
  if (
    cleanTargetBeforeParen &&
    (cleanOpt === cleanTargetBeforeParen || strippedOpt === cleanTargetBeforeParen)
  ) {
    return true;
  }

  return false;
}

export function findMatchingOptionId(options: { id: string; text: string }[], val: string): string {
  const found = options.find((o) => isOptionMatch(o, val));
  return found ? found.id : val;
}

export function sanitizeAnswersForDraft(
  questions: QuizQuestion[],
  answers: QuizAnswers,
): QuizAnswers {
  const result: QuizAnswers = {};
  for (const q of questions) {
    const vals = answers[q.id];
    if (!vals || vals.length === 0) continue;
    if (q.kind === "text") {
      if (q.blanks <= 1) {
        if (vals[0]?.trim()) result[q.id] = [vals[0].trim()];
      } else if (vals.length === q.blanks && vals.every((v) => v.trim().length > 0)) {
        result[q.id] = vals;
      }
    } else if (q.kind === "subjective") {
      if (vals[0]?.trim()) result[q.id] = [vals[0].trim()];
    } else {
      const valid = vals
        .map((v) => findMatchingOptionId(q.options, v))
        .filter((v) => q.options.some((o) => o.id === v));
      if (valid.length > 0) {
        if (q.kind === "single" && valid.length === 1) {
          result[q.id] = valid;
        } else if (q.kind === "multiple") {
          result[q.id] = valid;
        }
      }
    }
  }
  return result;
}

export function hasAnswersDifferences(a: QuizAnswers, b: QuizAnswers): boolean {
  const keysA = Object.keys(a);
  const keysB = Object.keys(b);
  if (keysA.length !== keysB.length) return true;
  for (const k of keysA) {
    const arrA = a[k] ?? [];
    const arrB = b[k] ?? [];
    if (arrA.length !== arrB.length) return true;
    for (let i = 0; i < arrA.length; i++) {
      if (arrA[i] !== arrB[i]) return true;
    }
  }
  return false;
}

export type QuizReview = {
  questionId: string;
  answers: string[];
  probabilities: Record<string, number>;
  message: string;
};

export interface QuizPanelProps {
  quiz: QuizState;
  dryRun?: boolean;
  weight?: number | null;
  onSnapshot?: (v: UoocSnapshot) => void;
  active?: boolean;
  onClose?: () => void;

  // Custom action overrides (for Exam / Homework / Custom Paper mode)
  isExam?: boolean;
  limitTime?: number; // In minutes, for countdown timer
  onMatchQa?: (
    questions: QuizQuestion[],
  ) => Promise<{ answers: QuizAnswers; reviews: QuizReview[] }>;
  onSolveAi?: (
    questions: QuizQuestion[],
    onSessionEntry?: (entry: AiSessionEntry) => void,
  ) => Promise<{ answers: QuizAnswers; reviews: QuizReview[] }>;
  onSubmit?: (answers: QuizAnswers) => Promise<void>;
  onSaveDraft?: (answers: QuizAnswers) => Promise<void>;
  onOpenPortal?: () => Promise<void>;
  onSyncPortal?: () => Promise<void>;
  onGenerateSubjective?: (question: QuizQuestion) => Promise<string>;
  extraHeaderActions?: React.ReactNode;
  warningAlert?: React.ReactNode;
}

export function QuizPanel({
  quiz,
  dryRun = false,
  weight,
  onSnapshot,
  active = false,
  onClose: _onClose,
  isExam = false,
  limitTime,
  onMatchQa,
  onSolveAi,
  onSubmit,
  onSaveDraft,
  onOpenPortal,
  onSyncPortal,
  onGenerateSubjective,
  warningAlert,
}: QuizPanelProps) {
  const cacheKey = getQuizCacheKey(quiz.accountId, quiz.cid, quiz.taskId, quiz.resourceId);
  const [answers, setAnswers] = useState<QuizAnswers>(() => {
    if (quiz.stage !== "passed") {
      const cached = loadQuizCache(cacheKey);
      if (cached && Object.keys(cached).length > 0) {
        return { ...quiz.answers, ...cached };
      }
    } else if (quiz.studentAnswers && Object.keys(quiz.studentAnswers).length > 0) {
      return { ...quiz.answers, ...quiz.studentAnswers };
    }
    return quiz.answers;
  });
  const [busy, setBusy] = useState(false);
  const [manual, setManual] = useState(false);
  const [reviews, setReviews] = useState<QuizReview[]>(quiz.reviews ?? []);
  const [confirmSubmitOpen, setConfirmSubmitOpen] = useState(false);

  useEffect(() => {
    setReviews(quiz.reviews ?? []);
  }, [quiz.reviews]);
  const [showHtmlTags, setShowHtmlTags] = useHtmlTagVisibility();
  const [actionModal, setActionModal] = useState<{
    type: "matchQa" | "generate" | "submit";
    title: string;
    step: number;
    completed: boolean;
    summary?: string;
  } | null>(null);

  const [liveSessions, setLiveSessions] = useState<AiSessionEntry[]>(quiz.aiSessions ?? []);
  const [portalAction, setPortalAction] = useState<"open" | "sync">("open");

  const [secondsRemaining, setSecondsRemaining] = useState<number>(() => {
    if (typeof limitTime === "number" && limitTime > 0) {
      return limitTime * 60;
    }
    return 0;
  });

  useEffect(() => {
    if (typeof limitTime === "number" && limitTime > 0) {
      setSecondsRemaining(limitTime * 60);
    }
  }, [limitTime]);

  useEffect(() => {
    if (quiz.stage === "passed" || !(typeof limitTime === "number" && limitTime > 0)) {
      return;
    }
    const timer = setInterval(() => {
      setSecondsRemaining((prev) => {
        if (prev <= 1) {
          clearInterval(timer);
          return 0;
        }
        return prev - 1;
      });
    }, 1000);

    let heartbeatTimer: ReturnType<typeof setInterval> | undefined;
    if (isExam && quiz.taskId) {
      void invoke("heartbeat_uooc_exam", { taskId: quiz.taskId }).catch(() => {});
      heartbeatTimer = setInterval(() => {
        void invoke("heartbeat_uooc_exam", { taskId: quiz.taskId }).catch(() => {});
      }, 30000);
    }

    return () => {
      clearInterval(timer);
      if (heartbeatTimer) clearInterval(heartbeatTimer);
    };
  }, [quiz.stage, limitTime, isExam, quiz.taskId]);

  useEffect(() => {
    setPortalAction("open");
  }, [quiz.cid, quiz.taskId]);

  const isDirtyRef = useRef(false);
  const submittedRef = useRef(false);
  const answersRef = useRef(answers);
  answersRef.current = answers;
  const quizRef = useRef(quiz);
  quizRef.current = quiz;
  const revisionRef = useRef(quiz.revision);
  useEffect(() => {
    revisionRef.current = quiz.revision;
  }, [quiz.revision]);
  const weightRef = useRef(weight);
  useEffect(() => {
    weightRef.current = weight;
  }, [weight]);
  const onSnapshotRef = useRef(onSnapshot);
  useEffect(() => {
    onSnapshotRef.current = onSnapshot;
  }, [onSnapshot]);
  const busyRef = useRef(busy);
  busyRef.current = busy;

  useEffect(() => {
    if (quiz.aiSessions && quiz.aiSessions.length > 0) {
      setLiveSessions(quiz.aiSessions);
    }
  }, [quiz.aiSessions]);

  useEffect(() => {
    let mounted = true;
    let unlistenFn: (() => void) | undefined;
    try {
      listen<AiSessionEntry>("uooc-ai-session-progress", (event) => {
        if (!mounted) return;
        const entry = event.payload;
        setLiveSessions((prev) => {
          const next = [...prev];
          const idx = next.findIndex((s) => s.batchIndex === entry.batchIndex);
          if (idx >= 0) {
            next[idx] = entry;
          } else {
            next.push(entry);
          }
          return next;
        });
      })
        .then((unlisten) => {
          if (!mounted) {
            unlisten();
          } else {
            unlistenFn = unlisten;
          }
        })
        .catch(() => {});
    } catch {
      // Non-tauri or mock environment safe fallback
    }
    return () => {
      mounted = false;
      unlistenFn?.();
    };
  }, []);

  // 监听 quiz 变化，自动将答案值规范化为对应选项的 option.id；当有题库标准答案或 AI 判定推荐答案时，自动切换选中该答案
  useEffect(() => {
    const isPassed = quiz.stage === "passed";
    const cached = !isPassed ? loadQuizCache(cacheKey) : null;
    const baseAnswers: QuizAnswers =
      isPassed && quiz.studentAnswers && Object.keys(quiz.studentAnswers).length > 0
        ? { ...quiz.answers, ...quiz.studentAnswers }
        : cached && Object.keys(cached).length > 0
          ? { ...quiz.answers, ...cached }
          : { ...quiz.answers };
    const nextAnswers: QuizAnswers = { ...baseAnswers };
    let hasAutoSelected = false;

    for (const q of quiz.questions) {
      if (isPassed) {
        const curr = nextAnswers[q.id];
        if (curr && curr.length > 0) {
          nextAnswers[q.id] = curr.map((val) => findMatchingOptionId(q.options, val));
        }
        continue;
      }

      // 1. 优先找题库匹配的标准答案 (来自题库 review 或已确认答案)
      const stdReview = quiz.reviews.find(
        (r) =>
          r.questionId === q.id &&
          (r.message.includes("题库") ||
            r.message.includes("标准答案") ||
            r.message.includes("已确认答案")) &&
          !r.message.includes("排除"),
      );
      // 2. 其次找 AI 判定推荐答案 (来自 AI review)
      const aiReview = quiz.reviews.find(
        (r) =>
          r.questionId === q.id &&
          !r.message.includes("题库") &&
          !r.message.includes("已确认答案") &&
          !r.message.includes("排除") &&
          r.answers.length > 0,
      );

      const targetReview = stdReview ?? aiReview;
      if (targetReview && targetReview.answers.length > 0) {
        const mapped = targetReview.answers.map((ans) => findMatchingOptionId(q.options, ans));
        const prev = nextAnswers[q.id];
        if (!prev || prev.length !== mapped.length || prev.some((v, i) => v !== mapped[i])) {
          nextAnswers[q.id] = mapped;
          hasAutoSelected = true;
        }
      } else {
        const curr = nextAnswers[q.id];
        if (curr && curr.length > 0) {
          nextAnswers[q.id] = curr.map((val) => findMatchingOptionId(q.options, val));
        }
      }
    }
    setAnswers(nextAnswers);

    if (!isPassed) {
      if (hasAutoSelected) {
        saveQuizCache(cacheKey, nextAnswers);
        isDirtyRef.current = true;
      } else if (cached && hasAnswersDifferences(cached, nextAnswers)) {
        isDirtyRef.current = true;
      }
    }
  }, [quiz.revision, quiz.answers, quiz.reviews, quiz.questions, cacheKey, quiz.stage]);

  // 当 modal 关闭时（组件卸载），若答案有更新且未提交，自动执行服务端保存
  useEffect(() => {
    return () => {
      if (submittedRef.current) return;
      if (busyRef.current) return;
      const currentQuiz = quizRef.current;
      if (!currentQuiz) return;
      if (currentQuiz.stage === "passed") return;
      if (currentQuiz.questions.length === 0) return;
      if (!isDirtyRef.current) return;

      const currentAnswers = answersRef.current;
      const sanitized = sanitizeAnswersForDraft(currentQuiz.questions, currentAnswers);

      if (onSaveDraft) {
        void onSaveDraft(sanitized)
          .then(() => {
            toast.success("草稿已自动保存");
          })
          .catch(() => {});
        return;
      }

      if (!(typeof weightRef.current === "number" && weightRef.current > 0)) return;

      void invoke<UoocSnapshot>("save_uooc_quiz_answers", {
        cid: currentQuiz.cid,
        revision: revisionRef.current,
        answers: sanitized,
      })
        .then((state) => {
          if (state) {
            onSnapshotRef.current?.(state);
            toast.success("测验草稿已自动保存");
          }
        })
        .catch(() => {});
    };
  }, [onSaveDraft]);

  const running =
    active && ["preparing", "generating", "submitting", "verifying"].includes(quiz.stage);
  const isCustomMode = Boolean(onMatchQa || onSolveAi || onSubmit || isExam);
  const isWeightDisabled = !isCustomMode && !(typeof weight === "number" && weight > 0);
  const disabled = busy || running || isWeightDisabled;

  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    try {
      await fn();
    } catch (e) {
      const msg =
        typeof e === "string" ? e : (e as { message?: string })?.message || "操作失败，请重试";
      toast.danger("操作失败", { description: msg });
      setActionModal(null);
    } finally {
      setBusy(false);
    }
  };

  const control = async (action: "generate" | "check" | "matchQa") => {
    const nextSnapshot = await invoke<UoocSnapshot>("control_uooc_quiz", {
      cid: quiz.cid,
      revision: revisionRef.current,
      action,
    });
    if (onSnapshot) onSnapshot(nextSnapshot);
    const nextQuiz = Object.values(nextSnapshot.quizzes ?? {}).find(
      (q) => q.accountId === quiz.accountId && q.cid === quiz.cid,
    );
    if (nextQuiz) {
      revisionRef.current = nextQuiz.revision;

      const updatedAnswers: QuizAnswers = { ...nextQuiz.answers };
      for (const q of nextQuiz.questions) {
        const stdRev = nextQuiz.reviews.find(
          (r) =>
            r.questionId === q.id &&
            (r.message.includes("题库") ||
              r.message.includes("标准答案") ||
              r.message.includes("已确认答案")) &&
            !r.message.includes("排除"),
        );
        const aiRev = nextQuiz.reviews.find(
          (r) =>
            r.questionId === q.id &&
            !r.message.includes("题库") &&
            !r.message.includes("已确认答案") &&
            !r.message.includes("排除") &&
            r.answers.length > 0,
        );
        const targetRev = stdRev ?? aiRev;
        if (targetRev && targetRev.answers.length > 0) {
          updatedAnswers[q.id] = targetRev.answers.map((ans) =>
            findMatchingOptionId(q.options, ans),
          );
        } else if (nextQuiz.answers[q.id]) {
          updatedAnswers[q.id] = nextQuiz.answers[q.id].map((ans) =>
            findMatchingOptionId(q.options, ans),
          );
        }
      }

      setAnswers(updatedAnswers);
      saveQuizCache(cacheKey, updatedAnswers);
      isDirtyRef.current = true;
      if (nextQuiz.aiSessions && nextQuiz.aiSessions.length > 0) {
        setLiveSessions(nextQuiz.aiSessions);
      }
    }
    return nextSnapshot;
  };

  const save = async () => {
    const sanitized = sanitizeAnswersForDraft(quiz.questions, answers);
    if (onSaveDraft) {
      await onSaveDraft(sanitized);
      isDirtyRef.current = false;
      saveQuizCache(cacheKey, answers);
      return { ...quiz, answers: sanitized };
    }
    const state = await invoke<UoocSnapshot>("save_uooc_quiz_answers", {
      cid: quiz.cid,
      revision: revisionRef.current,
      answers: sanitized,
    });
    isDirtyRef.current = false;
    saveQuizCache(cacheKey, answers);
    if (state) {
      if (onSnapshot) onSnapshot(state);
      const updated = Object.values(state?.quizzes ?? {}).find(
        (q) => q.accountId === quiz.accountId && q.cid === quiz.cid,
      );
      if (updated) {
        revisionRef.current = updated.revision;
      }
      return updated;
    }
    return undefined;
  };

  const set = (id: string, values: string[]) => {
    setAnswers((old) => {
      const next = { ...old, [id]: values };
      saveQuizCache(cacheKey, next);
      isDirtyRef.current = true;
      return next;
    });
  };

  const handleActionWithModal = async (
    action: "matchQa" | "generate" | "submit",
    title: string,
  ) => {
    if (action === "generate") {
      setLiveSessions([]);
    }
    setActionModal({ type: action, title, step: 2, completed: false });
    await run(async () => {
      if (action === "matchQa" && onMatchQa) {
        const result = await onMatchQa(quiz.questions);
        const nextAnswers = { ...answers, ...result.answers };
        for (const q of quiz.questions) {
          const matchedRev = result.reviews.find((r) => r.questionId === q.id);
          if (matchedRev && matchedRev.answers.length > 0) {
            nextAnswers[q.id] = matchedRev.answers.map((ans) =>
              findMatchingOptionId(q.options, ans),
            );
          }
        }
        setAnswers(nextAnswers);
        saveQuizCache(cacheKey, nextAnswers);
        isDirtyRef.current = true;

        const mergedReviews = [
          ...reviews.filter((r) => !result.reviews.some((nr) => nr.questionId === r.questionId)),
          ...result.reviews,
        ];
        setReviews(mergedReviews);
        setActionModal((prev) =>
          prev
            ? {
                ...prev,
                step: 3,
                completed: true,
                summary: `题库匹配完成，已命中 ${Object.keys(result.answers).length} 题答案`,
              }
            : null,
        );
        return;
      }

      if (action === "generate" && onSolveAi) {
        const confirmedQids = new Set(
          reviews
            .filter(
              (r) =>
                (r.message.includes("题库") || r.message.includes("标准答案")) &&
                !r.message.includes("排除"),
            )
            .map((r) => r.questionId),
        );
        const questionsForAi = quiz.questions.filter((q) => !confirmedQids.has(q.id));
        if (questionsForAi.length === 0) {
          setActionModal((prev) =>
            prev
              ? {
                  ...prev,
                  step: 3,
                  completed: true,
                  summary:
                    "全部题目已在题库中命中确认答案，无需向 AI 发起请求（已节省 Token 消耗）",
                }
              : null,
          );
          return;
        }

        const result = await onSolveAi(questionsForAi, (entry) => {
          setLiveSessions((prev) => {
            const next = [...prev];
            const idx = next.findIndex((s) => s.batchIndex === entry.batchIndex);
            if (idx >= 0) {
              next[idx] = entry;
            } else {
              next.push(entry);
            }
            return next;
          });
        });
        const nextAnswers = { ...answers };
        for (const [qid, ans] of Object.entries(result.answers)) {
          if (!confirmedQids.has(qid)) {
            const q = quiz.questions.find((item) => item.id === qid);
            if (q) {
              nextAnswers[qid] = ans.map((v) => findMatchingOptionId(q.options, v));
            } else {
              nextAnswers[qid] = ans;
            }
          }
        }
        setAnswers(nextAnswers);
        saveQuizCache(cacheKey, nextAnswers);
        isDirtyRef.current = true;

        const mergedReviews = [
          ...reviews.filter((r) => confirmedQids.has(r.questionId)),
          ...result.reviews.filter((r) => !confirmedQids.has(r.questionId)),
        ];
        setReviews(mergedReviews);
        setActionModal((prev) =>
          prev
            ? {
                ...prev,
                step: 3,
                completed: true,
                summary: `AI 智能答题完成，已为 ${Object.keys(result.answers).length} 题生成推荐作答`,
              }
            : null,
        );
        return;
      }

      if (action === "submit" && onSubmit) {
        submittedRef.current = true;
        try {
          const sanitized = sanitizeAnswersForDraft(quiz.questions, answers);
          await onSubmit(sanitized);
          clearQuizCache(cacheKey);
          setActionModal((prev) =>
            prev ? { ...prev, step: 3, completed: true, summary: "试卷已成功提交" } : null,
          );
          return;
        } catch (e) {
          submittedRef.current = false;
          throw e;
        }
      }

      // Default course quiz handling:
      let nextSnapshot: UoocSnapshot | undefined;
      if (action === "submit") {
        submittedRef.current = true;
        try {
          const saved = await save();
          if (!saved) throw new Error("草稿已失效");
          nextSnapshot = await invoke<UoocSnapshot>("control_uooc_quiz", {
            cid: quiz.cid,
            revision: saved.revision,
            action: "submit",
          });
          if (onSnapshot) onSnapshot(nextSnapshot);
          clearQuizCache(cacheKey);
        } catch (e) {
          submittedRef.current = false;
          throw e;
        }
      } else {
        nextSnapshot = await control(action);
      }
      const updatedQuiz = nextSnapshot
        ? Object.values(nextSnapshot.quizzes ?? {}).find(
            (q) => q.accountId === quiz.accountId && q.cid === quiz.cid,
          )
        : undefined;
      const summaryMsg = updatedQuiz?.message;
      setActionModal((prev) =>
        prev ? { ...prev, step: 3, completed: true, summary: summaryMsg } : null,
      );
    });
  };

  const handleOpenPortal = async () => {
    await run(async () => {
      if (onOpenPortal) {
        await onOpenPortal();
      } else {
        await invoke("open_uooc_quiz_portal", { cid: quiz.cid });
      }
      setPortalAction("sync");
    });
  };

  const handleSyncPortal = async () => {
    await run(async () => {
      if (onSyncPortal) {
        await onSyncPortal();
      } else {
        await control("check");
      }
      toast.success("已同步官网试卷数据");
    });
  };

  const handleUploadDraft = async () => {
    await run(async () => {
      const sanitized = sanitizeAnswersForDraft(quiz.questions, answers);
      if (onSaveDraft) {
        await onSaveDraft(sanitized);
        isDirtyRef.current = false;
        saveQuizCache(cacheKey, answers);
        toast.success("草稿已成功保存至官网");
        return;
      }
      await save();
      if (!dryRun && quiz.taskId) {
        await invoke("save_uooc_exam_draft", {
          cid: quiz.cid,
          taskId: quiz.taskId,
          answers: sanitized,
        });
        toast.success("草稿已成功上传并保存至官网");
      } else if (dryRun) {
        toast.info("预览模式：草稿已在本地保存，未上传至官网");
      } else {
        toast.success("草稿已保存");
      }
    });
  };

  const handleSubjectiveAi = async (q: QuizQuestion) => {
    await run(async () => {
      let generated = "";
      if (onGenerateSubjective) {
        generated = await onGenerateSubjective(q);
      } else {
        generated = await invoke<string>("generate_uooc_subjective", {
          stem: q.text,
          question: q,
          context: null,
          referenceExamples: null,
        });
      }
      set(q.id, [generated]);
      toast.success("AI 已生成作答草稿");
    });
  };

  // 判定并格式化题目类型标签（参考本地题库浏览模块）
  const resolveQuestionTypeLabel = (q: QuizQuestion): string => {
    if (q.kind === "subjective") return "简答/论述题";
    if (q.kind === "text") return "填空题";
    if (q.kind === "multiple") return "多选题";
    if (q.options.length === 2) {
      const t0 = q.options[0].text.trim().toLowerCase();
      const t1 = q.options[1].text.trim().toLowerCase();
      if (
        (t0.includes("正确") || t0.includes("对") || t0 === "true" || t0 === "t" || t0 === "a") &&
        (t1.includes("错误") || t1.includes("错") || t1 === "false" || t1 === "f" || t1 === "b")
      ) {
        return "判断题";
      }
    }
    return "单选题";
  };

  // 计算每个选项的标注状态（标准答案 / 排除答案 / AI判定 / 用户已选），独立于可变的用户临时编辑状态
  const getOptionBadges = (
    q: QuizQuestion,
    optId: string,
    optText: string,
    currentSelected: string[],
  ) => {
    const optObj = { id: optId, text: optText };

    // 1. 题库匹配的标准答案 (来自题库 review 或已通过状态的官方确认答案)
    const isPassed = quiz.stage === "passed";
    const passedAnswers = isPassed ? (quiz.answers[q.id] ?? []) : [];
    const passedMatch = passedAnswers.some((ans) => isOptionMatch(optObj, ans));

    const bankReview = reviews.find(
      (r) =>
        r.questionId === q.id &&
        (r.message.includes("题库") ||
          r.message.includes("已确认答案") ||
          r.message.includes("标准答案") ||
          r.message.includes("精确命中") ||
          r.message.includes("向量命中")) &&
        !r.message.includes("排除"),
    );
    const bankMatch = bankReview
      ? bankReview.answers.some((ans) => isOptionMatch(optObj, ans))
      : false;

    const isStandard = passedMatch || bankMatch;

    // 2. 题库排除答案 (来自题库排除 review)
    const excludeReview = reviews.find(
      (r) =>
        r.questionId === q.id && (r.message.includes("排除") || r.message.includes("请勿选择")),
    );
    const cleanOptText = stripHtml(optText);
    const isExcluded =
      !isStandard &&
      (excludeReview
        ? excludeReview.answers.some((ans) => isOptionMatch(optObj, ans)) ||
          excludeReview.message.includes(optId.toUpperCase()) ||
          (cleanOptText.length >= 2 && excludeReview.message.includes(cleanOptText))
        : false);

    // 3. AI 判定答案 (来自 AI review 推荐选项，非题库标准答案)
    const aiReview = reviews.find(
      (r) =>
        r.questionId === q.id &&
        !r.message.includes("题库") &&
        !r.message.includes("已确认答案") &&
        !r.message.includes("排除") &&
        r.answers.length > 0,
    );
    const isAiJudged =
      !isStandard &&
      !isExcluded &&
      (aiReview ? aiReview.answers.some((ans) => isOptionMatch(optObj, ans)) : false);

    // 4. 用户当前是否在界面上选择/勾选了该项（支持自动选择标准答案与 AI 判定答案）
    const isSelected =
      currentSelected.some((ans) => isOptionMatch(optObj, ans)) ||
      (currentSelected.length === 0 && (isStandard || isAiJudged));

    return {
      isStandard,
      isExcluded,
      isAiJudged,
      isSelected,
    };
  };

  // 确定单选题 / 判断题选中的 Radio value
  const getSelectedRadioValue = (q: QuizQuestion): string => {
    const current = answers[q.id]?.[0];
    if (current) {
      const matched = q.options.find((o) => isOptionMatch(o, current));
      if (matched) return matched.id;
      return current;
    }
    // 自动回退选中标准答案项或 AI 判定项
    for (const o of q.options) {
      const b = getOptionBadges(q, o.id, o.text, []);
      if (b.isStandard || b.isAiJudged) return o.id;
    }
    return "";
  };

  // 确定多选题选中的 Checkbox values 列表
  const getSelectedCheckboxValues = (q: QuizQuestion): string[] => {
    const current = answers[q.id];
    if (current && current.length > 0) {
      return current.map((val) => {
        const matched = q.options.find((o) => isOptionMatch(o, val));
        return matched ? matched.id : val;
      });
    }
    // 自动回退选中所有标准答案项或 AI 判定项
    const autoOpts = q.options
      .filter((o) => {
        const b = getOptionBadges(q, o.id, o.text, []);
        return b.isStandard || b.isAiJudged;
      })
      .map((o) => o.id);
    return autoOpts;
  };

  const isModalFinished = Boolean(actionModal?.completed && !busy);
  const unansweredCount = quiz.questions.filter((q) => {
    const val = answers[q.id];
    return !val || val.every((v) => !v.trim());
  }).length;

  return (
    <>
      {/* 4. 上面的 课程测验 <label>测验已完成</> <button>查看测验</> 等作为 Header */}
      <div className="shrink-0 border-b border-separator/40 p-4 sm:p-5">
        <div className="flex flex-wrap items-center justify-between gap-3 pr-8">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-base font-semibold">{quiz.title || "课程测验"}</span>
            {quiz.stage === "passed" && <Chip color="success">已完成</Chip>}
            {running && <LoadingIndicator size={16} aria-label="处理中" />}
          </div>
          <div className="flex flex-wrap items-center gap-3">
            {typeof limitTime === "number" && limitTime > 0 && quiz.stage !== "passed" && (
              <Chip
                color={secondsRemaining < 300 ? "danger" : "warning"}
                variant="soft"
                size="sm"
                className="font-mono text-xs font-semibold"
              >
                <LuClock className="mr-1 inline-block" />
                剩余时间: {Math.floor(secondsRemaining / 60)}:
                {(secondsRemaining % 60).toString().padStart(2, "0")}
              </Chip>
            )}
            <Switch
              checked={showHtmlTags}
              onChange={(e: any) => setShowHtmlTags(e.detail?.checked ?? e.target?.checked)}
              aria-label="显示 HTML 标签"
            >
              <span className="text-xs">HTML 标签</span>
            </Switch>
            {(quiz.stage === "passed" || quiz.message.includes("当前测验已由服务端确认完成")) &&
              !isCustomMode && (
                <Button
                  variant="tonal"
                  size="xs"
                  disabled={busy || running}
                  onClick={() =>
                    void run(async () => {
                      const next = await invoke<UoocSnapshot>("fetch_uooc_completed_quiz", {
                        cid: quiz.cid,
                        taskId: quiz.taskId || undefined,
                        resourceId: quiz.resourceId || undefined,
                      });
                      if (onSnapshot) onSnapshot(next);
                      toast.success("已成功拉取当前测验已完成的题目与答案");
                    })
                  }
                >
                  查看测验
                </Button>
              )}
          </div>
        </div>
        <p role="status" className="text-xs text-muted mt-1.5">
          {quiz.message || "试卷加载完成"}
        </p>
      </div>

      {/* 1. 题目与选项展示模块：仅 Body 随题目滚动；标注器显示在选项旁边，下方不再堆砌答案框 */}
      <div className="flex-1 overflow-y-auto p-4 sm:p-6 min-h-0 space-y-4">
        {warningAlert && <div className="mb-2">{warningAlert}</div>}
        {quiz.questions.length === 0 && (
          <div className="flex flex-col items-center justify-center p-8 space-y-3 text-center">
            <p className="text-sm text-muted">{quiz.message || "当前测验已完成"}</p>
            <div className="flex items-center gap-2">
              {!isCustomMode && (
                <Button
                  variant="tonal"
                  size="xs"
                  disabled={busy || running}
                  onClick={() =>
                    void run(async () => {
                      const next = await invoke<UoocSnapshot>("fetch_uooc_completed_quiz", {
                        cid: quiz.cid,
                        taskId: quiz.taskId || undefined,
                        resourceId: quiz.resourceId || undefined,
                      });
                      if (onSnapshot) onSnapshot(next);
                      toast.success("已成功拉取当前测验已完成的题目与答案");
                    })
                  }
                >
                  查看测验详情
                </Button>
              )}
              <Button
                variant="text"
                size="xs"
                disabled={busy || running}
                onClick={() => void handleOpenPortal()}
              >
                在官网打开试卷
              </Button>
            </div>
          </div>
        )}
        {(quiz.stage !== "awaitingChoice" || manual) &&
          quiz.questions.map((q, i) => {
            const typeLabel = resolveQuestionTypeLabel(q);

            // 针对填空题查找标准答案与AI判定
            const isPassed = quiz.stage === "passed";
            const passedAnswers = isPassed ? (quiz.answers[q.id] ?? []) : [];
            const bankReview = reviews.find(
              (r) =>
                r.questionId === q.id &&
                (r.message.includes("题库") ||
                  r.message.includes("已确认答案") ||
                  r.message.includes("标准答案") ||
                  r.message.includes("精确命中") ||
                  r.message.includes("向量命中")),
            );
            const aiReview = reviews.find(
              (r) =>
                r.questionId === q.id &&
                !r.message.includes("题库") &&
                !r.message.includes("已确认答案") &&
                r.answers.length > 0,
            );

            return (
              <div
                key={q.id}
                className="grid gap-3 rounded-xl border border-separator/50 bg-surface/50 p-4 transition-colors hover:border-separator"
              >
                {/* 题号与题目类型 Chip */}
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div className="flex items-center gap-2">
                    <span className="font-semibold text-sm text-foreground">第 {i + 1} 题</span>
                    <Chip size="sm" color="accent">
                      {typeLabel}
                    </Chip>
                  </div>
                  <span className="font-mono text-xs text-muted">ID: {q.id}</span>
                </div>

                {/* 题干文本（dim 暗淡虚化 HTML 标签） */}
                <p className="font-medium whitespace-pre-wrap text-foreground">
                  <DimmedHtmlText text={q.text} />
                </p>

                {/* 题目附带图片 */}
                {q.images.map((src, index) => (
                  <img
                    key={index}
                    src={src}
                    alt={`第 ${i + 1} 题图片 ${index + 1}`}
                    className="max-h-96 max-w-full rounded-lg border border-separator/40 object-contain"
                  />
                ))}

                {/* 2 & 3 & 4. 选项列表交互：标注器直接显示在选项旁边 */}
                {q.kind === "single" ? (
                  <Radios
                    aria-label={`第 ${i + 1} 题答案`}
                    value={getSelectedRadioValue(q)}
                    onChange={(e: any) => {
                      const val = e.detail?.value ?? e.target?.value;
                      if (val !== undefined && val !== "") set(q.id, [val]);
                    }}
                    disabled={disabled || quiz.stage === "passed"}
                    className="grid gap-2"
                  >
                    {q.options.map((o) => {
                      const badges = getOptionBadges(q, o.id, o.text, answers[q.id] ?? []);
                      const isRadioChecked = getSelectedRadioValue(q) === o.id;
                      const borderStyle = badges.isStandard
                        ? "border-success/50 bg-success/10"
                        : badges.isExcluded
                          ? "border-danger/40 bg-danger/5 opacity-85"
                          : badges.isAiJudged
                            ? "border-accent/50 bg-accent/10"
                            : isRadioChecked
                              ? "border-primary/50 bg-primary/10"
                              : "border-separator/40 bg-surface/30 hover:border-separator";

                      return (
                        <Radio
                          key={o.id}
                          value={o.id}
                          label={stripHtml(o.text) || o.id}
                          className={`flex items-start gap-2.5 rounded-lg border p-2.5 transition-colors cursor-pointer ${borderStyle}`}
                        >
                          <span
                            aria-hidden="true"
                            data-option-prefix={`${o.id.toUpperCase()}.`}
                            className="option-prefix font-bold text-foreground shrink-0 text-xs"
                          />
                          <span className="flex-1 text-xs text-foreground/90">
                            <DimmedHtmlText text={o.text} />
                          </span>

                          {/* 选项右侧标注器：标准答案 / 排除答案 / AI判定 / 用户已选 */}
                          <div className="flex flex-wrap items-center gap-1.5 shrink-0 ml-auto">
                            {badges.isStandard && (
                              <Chip size="sm" color="success">
                                标准答案
                              </Chip>
                            )}
                            {badges.isExcluded && (
                              <Chip size="sm" color="danger">
                                排除答案
                              </Chip>
                            )}
                            {badges.isAiJudged && (
                              <Chip size="sm" color="accent">
                                AI判定
                              </Chip>
                            )}
                            {isRadioChecked && (
                              <span className="inline-flex items-center gap-1 text-[11px] font-semibold text-primary">
                                <LuCircleCheck aria-hidden="true" /> 已选
                              </span>
                            )}
                          </div>
                        </Radio>
                      );
                    })}
                  </Radios>
                ) : q.kind === "multiple" ? (
                  <div role="group" aria-label={`第 ${i + 1} 题答案`} className="grid gap-2">
                    {q.options.map((o) => {
                      const badges = getOptionBadges(q, o.id, o.text, answers[q.id] ?? []);
                      const isBoxChecked = getSelectedCheckboxValues(q).includes(o.id);
                      const borderStyle = badges.isStandard
                        ? "border-success/50 bg-success/10"
                        : badges.isExcluded
                          ? "border-danger/40 bg-danger/5 opacity-85"
                          : badges.isAiJudged
                            ? "border-accent/50 bg-accent/10"
                            : isBoxChecked
                              ? "border-primary/50 bg-primary/10"
                              : "border-separator/40 bg-surface/30 hover:border-separator";

                      return (
                        <Checkbox
                          key={o.id}
                          value={o.id}
                          aria-label={stripHtml(o.text) || o.id}
                          checked={isBoxChecked}
                          disabled={disabled || quiz.stage === "passed"}
                          onChange={(e: any) => {
                            const checked = Boolean(e.detail?.checked ?? e.target?.checked);
                            const current = getSelectedCheckboxValues(q);
                            const next = checked
                              ? [...current.filter((x) => x !== o.id), o.id]
                              : current.filter((x) => x !== o.id);
                            set(q.id, next);
                          }}
                          className={`flex items-start gap-2.5 rounded-lg border p-2.5 transition-colors cursor-pointer ${borderStyle}`}
                        >
                          <span
                            aria-hidden="true"
                            data-option-prefix={`${o.id.toUpperCase()}.`}
                            className="option-prefix font-bold text-foreground shrink-0 text-xs"
                          />
                          <span className="flex-1 text-xs text-foreground/90">
                            <DimmedHtmlText text={o.text} />
                          </span>

                          {/* 选项右侧标注器：标准答案 / 排除答案 / AI判定 / 用户已选 */}
                          <div className="flex flex-wrap items-center gap-1.5 shrink-0 ml-auto">
                            {badges.isStandard && (
                              <Chip size="sm" color="success">
                                标准答案
                              </Chip>
                            )}
                            {badges.isExcluded && (
                              <Chip size="sm" color="danger">
                                排除答案
                              </Chip>
                            )}
                            {badges.isAiJudged && (
                              <Chip size="sm" color="accent">
                                AI判定
                              </Chip>
                            )}
                            {isBoxChecked && (
                              <span className="inline-flex items-center gap-1 text-[11px] font-semibold text-primary">
                                <LuCircleCheck aria-hidden="true" /> 已选
                              </span>
                            )}
                          </div>
                        </Checkbox>
                      );
                    })}
                  </div>
                ) : q.blanks > 1 ? (
                  Array.from({ length: q.blanks }, (_, blank) => {
                    const stdText = passedAnswers[blank] || bankReview?.answers[blank];
                    const aiText = aiReview?.answers[blank];

                    return (
                      <div key={blank} className="space-y-1.5">
                        <div className="flex items-center justify-between text-xs font-medium w-full">
                          <span>
                            第 {i + 1} 题 · 第 {blank + 1} 空
                          </span>
                          <div className="flex items-center gap-1.5 shrink-0">
                            {stdText && (
                              <Chip size="sm" color="success">
                                标准答案: {stripHtml(stdText)}
                              </Chip>
                            )}
                            {!stdText && aiText && (
                              <Chip size="sm" color="accent">
                                AI判定: {stripHtml(aiText)}
                              </Chip>
                            )}
                          </div>
                        </div>
                        <TextField
                          type="multiline"
                          label={`第 ${i + 1} 题 · 第 ${blank + 1} 空`}
                          value={answers[q.id]?.[blank] ?? ""}
                          disabled={disabled || quiz.stage === "passed"}
                          onChange={(event) => {
                            const values = Array.from(
                              { length: q.blanks },
                              (_, j) => answers[q.id]?.[j] ?? "",
                            );
                            values[blank] = event.detail.value;
                            set(q.id, values);
                          }}
                          className="w-full"
                        />
                      </div>
                    );
                  })
                ) : (
                  <div className="space-y-1.5">
                    <div className="flex items-center justify-between text-xs font-medium w-full">
                      <span>第 {i + 1} 题作答</span>
                      <div className="flex items-center gap-1.5 shrink-0">
                        {quiz.stage !== "passed" && (
                          <Button
                            size="xs"
                            variant="tonal"
                            disabled={disabled}
                            onClick={() => void handleSubjectiveAi(q)}
                          >
                            <LuSparkles className="mr-1 text-accent size-3.5" /> AI 解答
                          </Button>
                        )}
                        {bankReview?.answers[0] && (
                          <Chip size="sm" color="success">
                            标准答案: {stripHtml(bankReview.answers[0])}
                          </Chip>
                        )}
                        {!bankReview?.answers[0] && aiReview?.answers[0] && (
                          <Chip size="sm" color="accent">
                            AI判定: {stripHtml(aiReview.answers[0])}
                          </Chip>
                        )}
                      </div>
                    </div>
                    <TextField
                      type="multiline"
                      label={`第 ${i + 1} 题作答`}
                      value={answers[q.id]?.[0] ?? ""}
                      disabled={disabled || quiz.stage === "passed"}
                      onChange={(event: any) =>
                        set(q.id, [event.detail?.value ?? event.target?.value ?? ""])
                      }
                      className="w-full"
                    />
                  </div>
                )}

                {/* 题库命中说明 / AI 推荐解析 / Jev 概率分布或错误排除说明（如果有） */}
                {reviews
                  .filter(
                    (r) =>
                      r.questionId === q.id &&
                      (r.message.length > 0 || Object.keys(r.probabilities).length > 0),
                  )
                  .map((review, j) => (
                    <div
                      key={j}
                      className={`grid gap-1 rounded-lg border p-2.5 text-xs ${
                        review.message.includes("排除")
                          ? "border-danger/30 bg-danger/5 text-danger"
                          : review.message.includes("题库") || review.message.includes("标准答案")
                            ? "border-success/35 bg-success/10 text-foreground"
                            : review.message.includes("理由") || review.message.includes("解析")
                              ? "border-accent/35 bg-accent/10 text-foreground"
                              : "border-accent/25 bg-accent/5 text-muted"
                      }`}
                    >
                      <p className="font-medium text-foreground whitespace-pre-wrap">
                        {review.message}
                      </p>
                      {Object.keys(review.probabilities).length > 0 && (
                        <p className="text-[11px] text-muted">
                          概率分布：
                          {Object.entries(review.probabilities)
                            .map(([id, p]) => {
                              const opt = q.options.find((o) => o.id === id);
                              return `${opt ? opt.id.toUpperCase() : id}: ${(p * 100).toFixed(1)}%`;
                            })
                            .join(" · ")}
                        </p>
                      )}
                    </div>
                  ))}
              </div>
            );
          })}
      </div>

      {/* 3. 操作按钮作为 Footer，固定在底部不随题目滚动 */}
      {quiz.taskId && (
        <div className="shrink-0 border-t border-separator/40 p-3 sm:p-4 flex flex-wrap items-center justify-between gap-2 bg-surface/90 backdrop-blur">
          <div className="flex items-center gap-2 text-xs text-muted">
            {isExam && quiz.stage !== "passed" && (
              <span>
                未作答: {quiz.questions.filter((q) => (answers[q.id]?.length ?? 0) === 0).length} /{" "}
                {quiz.questions.length} 题
              </span>
            )}
          </div>

          <div className="flex flex-wrap items-center gap-2">
            {quiz.stage === "awaitingChoice" && (
              <Button variant="tonal" disabled={disabled} onClick={() => setManual(true)}>
                自主答题
              </Button>
            )}

            <ButtonGroup variant="secondary">
              <Button
                disabled={disabled || quiz.questions.length === 0 || quiz.stage === "passed"}
                onClick={() => void handleActionWithModal("matchQa", "题库匹配流程")}
              >
                匹配题库
              </Button>
              <Button
                disabled={disabled || quiz.questions.length === 0 || quiz.stage === "passed"}
                onClick={() => void handleActionWithModal("generate", "AI 智能答题流程")}
              >
                {isExam ? "AI 智能答题" : "AI 答题"}
              </Button>
            </ButtonGroup>

            <ButtonGroup>
              <Button
                variant="tonal"
                disabled={disabled || quiz.questions.length === 0 || quiz.stage === "passed"}
                onClick={() => void handleUploadDraft()}
              >
                {isExam ? "保存草稿" : "上传草稿"}
              </Button>
              <Button
                variant="filled"
                disabled={
                  disabled || dryRun || quiz.questions.length === 0 || quiz.stage === "passed"
                }
                onClick={() => {
                  if (isExam) {
                    setConfirmSubmitOpen(true);
                  } else {
                    void handleActionWithModal("submit", "测验提交与确认流程");
                  }
                }}
              >
                {isExam ? "提交试卷" : "提交答案"}
              </Button>
            </ButtonGroup>

            <div className="flex items-center gap-1">
              <Button
                variant="text"
                disabled={busy || running}
                onClick={() =>
                  void (portalAction === "open" ? handleOpenPortal() : handleSyncPortal())
                }
              >
                {portalAction === "open" ? "在官网打开试卷" : "同步官网试卷数据"}
              </Button>
              <Button
                variant="text"
                disabled={busy || running}
                onClick={() =>
                  void (portalAction === "open" ? handleSyncPortal() : handleOpenPortal())
                }
              >
                {portalAction === "open" ? "同步官网" : "在官网打开"}
              </Button>
            </div>
          </div>
        </div>
      )}

      {dryRun && <p className="text-xs text-muted px-4 pb-2">预览模式：草稿不会提交到官网。</p>}

      {/* 2. 点击匹配题库、AI 答题等时弹出的运行流程与会话调用 Modal */}
      {Boolean(actionModal) &&
        typeof document !== "undefined" &&
        createPortal(
          <Dialog
            open={Boolean(actionModal)}
            headline={actionModal?.title || "测验运行流程"}
            className="max-w-2xl"
            onClose={() => {
              if (isModalFinished) setActionModal(null);
            }}
          >
            <div className="grid gap-4 p-4 text-sm max-h-[70vh] overflow-y-auto">
              <div className="rounded-xl border border-separator/50 bg-surface/40 p-4 grid gap-2.5">
                <div className="flex items-center justify-between text-xs text-muted">
                  <span>当前执行状态</span>
                  <span
                    className={`font-mono ${isModalFinished ? "text-success font-semibold" : "text-accent"}`}
                  >
                    {isModalFinished ? "已完成" : "进行中"}
                  </span>
                </div>
                <p className="font-medium text-foreground">
                  {actionModal?.summary ||
                    quiz.message ||
                    (isModalFinished ? "流程已处理完成" : "正在处理测验任务...")}
                </p>
                {!isModalFinished && (
                  <Progress variant="linear" indeterminate aria-label="处理中" />
                )}
              </div>

              <div className="grid gap-2 text-xs">
                <span className="font-semibold text-muted">执行流程步骤：</span>
                <div className="grid gap-2.5 pl-2 border-l-2 border-separator/60">
                  <div className="flex items-center gap-2">
                    <span className="size-4 rounded-full bg-success/20 text-success flex items-center justify-center text-[10px] font-bold">
                      ✓
                    </span>
                    <span className="text-foreground">
                      已提取试卷结构（共 {quiz.questions.length} 道题目）
                    </span>
                  </div>

                  <div className="flex items-center gap-2">
                    {actionModal?.type === "matchQa" ? (
                      <>
                        {!isModalFinished ? (
                          <span className="size-4 rounded-full bg-accent/20 text-accent flex items-center justify-center text-[10px] animate-spin font-bold">
                            ●
                          </span>
                        ) : (
                          <span className="size-4 rounded-full bg-success/20 text-success flex items-center justify-center text-[10px] font-bold">
                            ✓
                          </span>
                        )}
                        <span className="flex items-center gap-1 text-foreground">
                          <LuDatabase aria-hidden="true" />{" "}
                          {isModalFinished
                            ? "已完成题库平台比对与答案匹配"
                            : "连接题库平台比对特征向量与答案"}
                        </span>
                      </>
                    ) : actionModal?.type === "generate" ? (
                      <>
                        {!isModalFinished ? (
                          <span className="size-4 rounded-full bg-accent/20 text-accent flex items-center justify-center text-[10px] animate-spin font-bold">
                            ●
                          </span>
                        ) : (
                          <span className="size-4 rounded-full bg-success/20 text-success flex items-center justify-center text-[10px] font-bold">
                            ✓
                          </span>
                        )}
                        <span className="flex items-center gap-1 text-foreground">
                          <LuBot aria-hidden="true" />{" "}
                          {isModalFinished
                            ? liveSessions.length === 0
                              ? "题库已全量覆盖，无需向 AI 发起请求"
                              : "调用 AI 模型计算题目语义与选项概率已完成"
                            : "调用 AI 模型计算题目语义与选项概率"}
                        </span>
                      </>
                    ) : (
                      <>
                        {!isModalFinished ? (
                          <span className="size-4 rounded-full bg-accent/20 text-accent flex items-center justify-center text-[10px] animate-spin font-bold">
                            ●
                          </span>
                        ) : (
                          <span className="size-4 rounded-full bg-success/20 text-success flex items-center justify-center text-[10px] font-bold">
                            ✓
                          </span>
                        )}
                        <span className="flex items-center gap-1 text-foreground">
                          <LuSparkles aria-hidden="true" />{" "}
                          {isModalFinished
                            ? "答案草稿已提交至服务端判题确认"
                            : "提交答案草稿至服务端判题确认"}
                        </span>
                      </>
                    )}
                  </div>

                  <div className="flex items-center gap-2">
                    {!isModalFinished ? (
                      <span className="size-4 rounded-full bg-muted/20 text-muted flex items-center justify-center text-[10px] font-bold">
                        3
                      </span>
                    ) : (
                      <span className="size-4 rounded-full bg-success/20 text-success flex items-center justify-center text-[10px] font-bold">
                        ✓
                      </span>
                    )}
                    <span className="text-foreground">完成防乱序选项对齐并更新试卷作答</span>
                  </div>
                </div>
              </div>

              {/* 1 & 2. AI 会话调用 session 流式与分批合并展示 */}
              {actionModal?.type === "generate" && (
                <div className="rounded-xl border border-separator/50 bg-surface/30 p-3.5 grid gap-3 mt-1">
                  <div className="flex items-center justify-between text-xs font-semibold text-foreground">
                    <span className="flex items-center gap-1.5">
                      <LuBot className="text-accent text-sm" aria-hidden="true" /> AI 会话调用
                      Session
                      {liveSessions.length > 0 ? ` (批次数量: ${liveSessions.length})` : ""}
                    </span>
                    {liveSessions[0]?.model && (
                      <span className="font-mono text-muted text-[11px]">
                        模型: {liveSessions[0].model}
                      </span>
                    )}
                  </div>

                  {liveSessions.length > 0 ? (
                    <div className="max-h-64 overflow-y-auto space-y-2.5 pr-1">
                      {liveSessions.map((session, sIdx) => {
                        const isBatchPending = session.status === "pending";
                        const isBatchError = session.status === "error";

                        return (
                          <details
                            key={session.batchIndex || sIdx}
                            className={`rounded-lg border p-2.5 text-xs grid gap-1.5 transition-colors ${
                              isBatchPending
                                ? "border-accent/40 bg-accent/5 animate-pulse"
                                : isBatchError
                                  ? "border-danger/40 bg-danger/5"
                                  : "border-separator/40 bg-surface/50 open:border-accent/40"
                            }`}
                            open={isBatchPending || sIdx === liveSessions.length - 1}
                          >
                            <summary className="font-medium text-foreground cursor-pointer flex items-center justify-between select-none">
                              <div className="flex items-center gap-2">
                                {isBatchPending ? (
                                  <LoadingIndicator size={16} aria-label="处理中" />
                                ) : isBatchError ? (
                                  <span className="text-danger font-bold text-xs">✕</span>
                                ) : (
                                  <span className="text-success font-bold text-xs">✓</span>
                                )}
                                <span className="font-semibold">
                                  批次 {session.batchIndex} (共 {session.questionIds?.length || 1}{" "}
                                  题)
                                </span>
                              </div>
                              <span className="text-[11px] font-mono text-accent shrink-0">
                                {isBatchPending
                                  ? session.response
                                    ? "正在实时响应与工具调用..."
                                    : "正在请求模型..."
                                  : isBatchError
                                    ? "调用失败"
                                    : `已生成 ${Object.keys(session.answers || {}).length} 题答案`}
                              </span>
                            </summary>

                            <div className="mt-1 text-[11px] text-muted">
                              <span>包含题目：</span>
                              <span className="font-mono">
                                {session.questionTitles?.join("、 ") ||
                                  session.questionIds?.join(", ")}
                              </span>
                            </div>

                            <div className="grid gap-2 pt-2 border-t border-separator/30 text-[11px]">
                              <div>
                                <span className="font-semibold text-muted">
                                  发送给 AI 的提示词 (Prompt):
                                </span>
                                <pre className="mt-1 p-2 rounded bg-background/80 font-mono text-[10px] overflow-x-auto whitespace-pre-wrap text-muted max-h-36">
                                  {session.prompt}
                                </pre>
                              </div>

                              <div>
                                <div className="flex items-center justify-between font-semibold text-muted mb-1">
                                  <span>
                                    {isBatchPending
                                      ? "AI 实时响应与工具调用 (Live):"
                                      : "AI 原始回复内容 (Response):"}
                                  </span>
                                  {isBatchPending && (
                                    <span className="flex items-center gap-1.5 text-accent text-[10px] font-normal">
                                      <LoadingIndicator size={16} /> 实时交互中...
                                    </span>
                                  )}
                                </div>
                                {session.response ? (
                                  <pre className="p-2 rounded bg-background/80 font-mono text-[10px] overflow-x-auto whitespace-pre-wrap text-accent font-medium max-h-48 scroll-smooth border border-accent/20">
                                    {session.response}
                                  </pre>
                                ) : isBatchPending ? (
                                  <div className="flex items-center gap-2 text-muted italic text-[11px] py-1.5 p-2 rounded bg-background/40">
                                    <LoadingIndicator size={16} />{" "}
                                    正在向模型发起请求，等待首字或工具调用...
                                  </div>
                                ) : null}
                              </div>

                              {session.error && (
                                <div className="text-danger text-[11px] p-2 rounded bg-danger/10 border border-danger/20">
                                  错误: {session.error}
                                </div>
                              )}
                            </div>
                          </details>
                        );
                      })}
                    </div>
                  ) : isModalFinished ? (
                    <div className="text-xs text-success flex items-center gap-2 p-3 bg-success/10 border border-success/30 rounded-lg">
                      <LuCircleCheck className="text-success shrink-0" aria-hidden="true" />
                      <span>
                        {quiz.questions.length > 0
                          ? "全部题目已在题库中命中确认答案，无需向 AI 发起请求（已节省 Token 消耗）。"
                          : "本测验试卷题目列表为空，无需向 AI 发起请求。"}
                      </span>
                    </div>
                  ) : (
                    <div className="text-xs text-muted font-mono flex items-center gap-2 p-3 bg-surface/40 rounded-lg">
                      <LoadingIndicator size={16} /> 正在准备向 AI 模型发送测验题目并等待会话响应...
                    </div>
                  )}
                </div>
              )}
            </div>
            <div slot="actions">
              <Button
                variant="filled"
                disabled={!isModalFinished}
                onClick={() => setActionModal(null)}
              >
                {!isModalFinished
                  ? "正在处理中..."
                  : isExam
                    ? "确认并返回作答"
                    : "查看试卷与作答结果"}
              </Button>
            </div>
          </Dialog>,
          document.body,
        )}
      {/* 确认交卷弹窗 (用于考试模式) */}
      {confirmSubmitOpen &&
        typeof document !== "undefined" &&
        createPortal(
          <Dialog
            open={confirmSubmitOpen}
            headline="确认提交试卷？"
            className="max-w-md"
            onClose={() => setConfirmSubmitOpen(false)}
          >
            <div className="space-y-3 p-4">
              <p className="text-sm font-medium">提交后将记录最终成绩并不可撤销。</p>
              {unansweredCount > 0 ? (
                <Banner status="danger">
                  <Banner.Title>仍有未作答题目！</Banner.Title>
                  <Banner.Description>
                    当前试卷共 {quiz.questions.length} 题，仍有 {unansweredCount} 题未填写答案。
                  </Banner.Description>
                </Banner>
              ) : (
                <Banner status="success">
                  <Banner.Title>所有题目均已作答</Banner.Title>
                  <Banner.Description>
                    共 {quiz.questions.length} 题，确认提交吗？
                  </Banner.Description>
                </Banner>
              )}
            </div>
            <div slot="actions" className="flex justify-end gap-2">
              <Button variant="tonal" onClick={() => setConfirmSubmitOpen(false)}>
                取消
              </Button>
              <Button
                className="button-danger"
                variant="filled"
                disabled={disabled}
                onClick={() => {
                  setConfirmSubmitOpen(false);
                  void handleActionWithModal("submit", "试卷提交流程");
                }}
              >
                确认提交
              </Button>
            </div>
          </Dialog>,
          document.body,
        )}
    </>
  );
}
