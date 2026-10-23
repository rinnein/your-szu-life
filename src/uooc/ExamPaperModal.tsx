import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Alert,
  AlertDialog,
  Button,
  Card,
  Checkbox,
  CheckboxGroup,
  Chip,
  Label,
  Modal,
  Radio,
  RadioGroup,
  Spinner,
  Switch,
  TextArea,
  TextField,
  toast,
} from "@heroui/react";
import {
  LuCircleCheck,
  LuClock,
  LuDatabase,
  LuExternalLink,
  LuSave,
  LuSend,
  LuSparkles,
} from "react-icons/lu";
import { DimmedHtmlText, useHtmlTagVisibility } from "./DimmedHtmlText";
import { findMatchingOptionId, isOptionMatch, stripHtml } from "./QuizPanel";
import type { ExamPaper, ExamTask, HomeworkTask, QuizAnswers, QuizQuestion } from "./types";

interface ExamPaperModalProps {
  exam: ExamTask | HomeworkTask | null;
  isOpen: boolean;
  onClose: () => void;
  onSubmitted?: () => void;
  isHomework?: boolean;
}

export function ExamPaperModal({ exam, isOpen, onClose, onSubmitted }: ExamPaperModalProps) {
  const [loading, setLoading] = useState(false);
  const [paper, setPaper] = useState<ExamPaper | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [answers, setAnswers] = useState<QuizAnswers>({});
  const [saving, setSaving] = useState(false);
  const [matchingQa, setMatchingQa] = useState(false);
  const [solvingAi, setSolvingAi] = useState(false);
  const [manual, setManual] = useState(false);
  const [showHtmlTags, setShowHtmlTags] = useHtmlTagVisibility();
  const [lastSavedTime, setLastSavedTime] = useState<string | null>(null);
  const [confirmSubmitOpen, setConfirmSubmitOpen] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [generatingAiQid, setGeneratingAiQid] = useState<string | null>(null);

  // 倒计时（秒）
  const [secondsRemaining, setSecondsRemaining] = useState<number | null>(null);

  const answersRef = useRef(answers);
  answersRef.current = answers;

  // 1. 打开弹窗时加载试卷
  useEffect(() => {
    if (!isOpen || !exam) {
      setPaper(null);
      setError(null);
      setAnswers({});
      setLastSavedTime(null);
      setSecondsRemaining(null);
      setManual(false);
      return;
    }

    let isMounted = true;
    setLoading(true);
    setError(null);

    invoke<ExamPaper>("get_uooc_exam_paper", { cid: exam.cid, taskId: exam.taskId })
      .then((res) => {
        if (!isMounted) return;
        setPaper(res);
        const initialAnswers = { ...(res.answers || {}) };

        // 题库命中或已确认答案时自动填充预选
        if (!res.isSubmitted && res.reviews && res.reviews.length > 0) {
          for (const q of res.questions) {
            const currentVals = initialAnswers[q.id];
            if (!currentVals || currentVals.length === 0) {
              const matchedRev = res.reviews.find(
                (r) => r.questionId === q.id && r.answers && r.answers.length > 0,
              );
              if (matchedRev) {
                initialAnswers[q.id] = matchedRev.answers.map((ans) =>
                  findMatchingOptionId(q.options, ans),
                );
              }
            }
          }
        }
        setAnswers(initialAnswers);

        const limitMin = res.limitTime || ("limitTime" in exam ? exam.limitTime : 0) || 0;
        if (limitMin > 0 && !res.isSubmitted) {
          setSecondsRemaining(limitMin * 60);
        }
      })
      .catch((err: unknown) => {
        if (!isMounted) return;
        const msg = typeof err === "string" ? err : "无法加载考试试卷";
        setError(msg);
      })
      .finally(() => {
        if (isMounted) setLoading(false);
      });

    return () => {
      isMounted = false;
    };
  }, [isOpen, exam]);

  // 2. 防作弊 30 秒轮询心跳（只在未交卷且试卷加载成功时运行）
  useEffect(() => {
    if (!isOpen || !exam || !paper || paper.isSubmitted) return;

    const heartbeatTimer = setInterval(() => {
      invoke("heartbeat_uooc_exam", { cid: exam.cid, taskId: exam.taskId }).catch(() => {
        // 心跳静默保持正常，不打扰用户正常作答
      });
    }, 30000);

    return () => {
      clearInterval(heartbeatTimer);
    };
  }, [isOpen, exam, paper]);

  // 3. 倒计时定时器
  useEffect(() => {
    if (secondsRemaining === null || secondsRemaining <= 0 || paper?.isSubmitted) return;

    const timer = setInterval(() => {
      setSecondsRemaining((prev) => (prev !== null && prev > 0 ? prev - 1 : 0));
    }, 1000);

    return () => {
      clearInterval(timer);
    };
  }, [secondsRemaining, paper?.isSubmitted]);

  // 4. 定时 60 秒自动暂存草稿
  useEffect(() => {
    if (!isOpen || !exam || !paper || paper.isSubmitted) return;

    const saveTimer = setInterval(() => {
      const current = answersRef.current;
      if (Object.keys(current).length === 0) return;

      invoke("save_uooc_exam_draft", {
        cid: exam.cid,
        taskId: exam.taskId,
        answers: current,
      })
        .then(() => {
          const now = new Date();
          const timeStr = now.toTimeString().split(" ")[0];
          setLastSavedTime(timeStr);
        })
        .catch(() => {});
    }, 60000);

    return () => {
      clearInterval(saveTimer);
    };
  }, [isOpen, exam, paper]);

  // 计算每个选项的标注状态（标准答案 / 排除 / AI判定 / 用户已选）
  const getOptionBadges = (
    q: QuizQuestion,
    optId: string,
    optText: string,
    currentSelected: string[],
  ) => {
    const optObj = { id: optId, text: optText };
    const reviews = paper?.reviews || [];

    // 1. 题库标准答案命中
    const bankReview = reviews.find(
      (r) =>
        r.questionId === q.id &&
        (r.message.includes("题库") ||
          r.message.includes("标准答案") ||
          r.message.includes("已确认答案") ||
          r.message.includes("精准命中") ||
          r.message.includes("向量命中")) &&
        !r.message.includes("排除"),
    );
    const isStandard = bankReview
      ? bankReview.answers.some((ans) => isOptionMatch(optObj, ans))
      : false;

    // 2. 排除答案
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

    // 3. AI 判定答案
    const aiReview = reviews.find(
      (r) =>
        r.questionId === q.id &&
        (r.message.includes("AI") || r.message.includes("判定")) &&
        !r.message.includes("题库") &&
        !r.message.includes("排除") &&
        r.answers.length > 0,
    );
    const isAiJudged =
      !isStandard &&
      !isExcluded &&
      (aiReview ? aiReview.answers.some((ans) => isOptionMatch(optObj, ans)) : false);

    // 4. 用户是否选中
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

  // 一键匹配题库
  const handleMatchQa = async () => {
    if (!paper || !exam) return;
    setMatchingQa(true);
    try {
      const res = await invoke<[QuizAnswers, ExamPaper["reviews"]]>("match_uooc_paper_qa", {
        questions: paper.questions,
      });
      if (res) {
        const [matchedAnswers, newReviews] = res;
        setPaper((prev) => (prev ? { ...prev, reviews: newReviews } : prev));
        const nextAnswers: QuizAnswers = { ...answers };
        for (const [qid, ansList] of Object.entries(matchedAnswers)) {
          const q = paper.questions.find((item) => item.id === qid);
          if (q) {
            nextAnswers[qid] = ansList.map((ans) => findMatchingOptionId(q.options, ans));
          }
        }
        setAnswers(nextAnswers);
        toast.success("题库单题匹配完成，已更新选项提示与参考答案");
      }
    } catch (err: unknown) {
      const msg = typeof err === "string" ? err : "题库匹配失败";
      toast.danger(msg);
    } finally {
      setMatchingQa(false);
    }
  };

  // 一键 AI 智能答题
  const handleSolveAi = async () => {
    if (!paper || !exam) return;
    setSolvingAi(true);
    try {
      const res = await invoke<[QuizAnswers, ExamPaper["reviews"]]>("solve_uooc_paper_ai", {
        questions: paper.questions,
      });
      if (res) {
        const [aiAnswers, newReviews] = res;
        setPaper((prev) => {
          if (!prev) return prev;
          const mergedReviews = [...prev.reviews];
          for (const r of newReviews) {
            const idx = mergedReviews.findIndex((ex) => ex.questionId === r.questionId);
            if (idx >= 0) {
              mergedReviews[idx] = r;
            } else {
              mergedReviews.push(r);
            }
          }
          return { ...prev, reviews: mergedReviews };
        });
        const nextAnswers: QuizAnswers = { ...answers };
        for (const [qid, ansList] of Object.entries(aiAnswers)) {
          const q = paper.questions.find((item) => item.id === qid);
          if (q) {
            if (q.kind === "subjective" || q.kind === "text") {
              nextAnswers[qid] = ansList;
            } else {
              nextAnswers[qid] = ansList.map((ans) => findMatchingOptionId(q.options, ans));
            }
          }
        }
        setAnswers(nextAnswers);
        toast.success("AI 智能答题完成，已填充推荐选项与主观题解答");
      }
    } catch (err: unknown) {
      const msg = typeof err === "string" ? err : "AI 答题失败，请检查模型设置";
      toast.danger(msg);
    } finally {
      setSolvingAi(false);
    }
  };

  // 单道主观题 AI 解答
  const handleGenerateSubjective = async (q: QuizQuestion) => {
    if (!paper || !exam) return;
    setGeneratingAiQid(q.id);
    try {
      const qReviews = (paper.reviews || []).filter((r) => r.questionId === q.id);
      const refExamples = qReviews.map((r) => ({
        questionId: r.questionId,
        stem: q.text,
        answers: r.answers,
        similarity: 0.9,
      }));
      const generated = await invoke<string>("generate_uooc_subjective", {
        stem: q.text,
        context: null,
        referenceExamples: refExamples,
      });
      if (generated && generated.trim().length > 0) {
        setAnswers((prev) => ({
          ...prev,
          [q.id]: [generated.trim()],
        }));
        toast.success("AI 已生成主观题作答草稿");
      }
    } catch (err: unknown) {
      const msg = typeof err === "string" ? err : "AI 解题失败，请检查模型设置";
      toast.danger(msg);
    } finally {
      setGeneratingAiQid(null);
    }
  };

  // 手动暂存草稿
  const handleSaveDraft = async () => {
    if (!exam || !paper) return;
    setSaving(true);
    try {
      await invoke("save_uooc_exam_draft", {
        cid: exam.cid,
        taskId: exam.taskId,
        answers,
      });
      const now = new Date();
      const timeStr = now.toTimeString().split(" ")[0];
      setLastSavedTime(timeStr);
      toast.success("草稿暂存成功");
    } catch (err: unknown) {
      const msg = typeof err === "string" ? err : "草稿暂存失败";
      toast.danger(msg);
    } finally {
      setSaving(false);
    }
  };

  // 确认并提交试卷
  const handleConfirmSubmit = async () => {
    if (!exam || !paper) return;
    setSubmitting(true);
    try {
      const msg = await invoke<string>("submit_uooc_exam", {
        cid: exam.cid,
        taskId: exam.taskId,
        answers,
      });
      toast.success(msg || "试卷已成功提交");
      setConfirmSubmitOpen(false);
      onSubmitted?.();
      onClose();
    } catch (err: unknown) {
      const msg = typeof err === "string" ? err : "提交试卷失败";
      toast.danger(msg);
      setConfirmSubmitOpen(false);
    } finally {
      setSubmitting(false);
    }
  };

  const formatTime = (secs: number) => {
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
  };

  const questions = paper?.questions || [];
  const answeredCount = questions.filter((q) => {
    const vals = answers[q.id];
    return Array.isArray(vals) && vals.length > 0 && vals.some((v) => v.trim().length > 0);
  }).length;
  const unansweredIndices = questions
    .map((q, idx) => {
      const vals = answers[q.id];
      const hasAnswer =
        Array.isArray(vals) && vals.length > 0 && vals.some((v) => v.trim().length > 0);
      return hasAnswer ? null : idx + 1;
    })
    .filter((v): v is number => v !== null);

  const canEdit = !paper?.isSubmitted || manual;

  return (
    <>
      <Modal isOpen={isOpen} onOpenChange={(open) => !open && onClose()}>
        <Modal.Backdrop>
          <Modal.Container size="cover" className="max-w-4xl">
            <Modal.Dialog
              aria-label="课程考核答题面板"
              className="flex h-[92vh] max-h-[92vh] flex-col overflow-hidden"
            >
              <Modal.CloseTrigger aria-label="关闭答题面板" />
              <Modal.Header className="flex flex-wrap items-center justify-between border-b pb-3 pr-10">
                <div className="space-y-1">
                  <Modal.Heading className="text-xl font-bold">
                    {paper?.title || exam?.title || "课程考核"}
                  </Modal.Heading>
                  <p className="text-xs text-muted">
                    {exam?.courseName} · 任务ID: {exam?.taskId}
                  </p>
                </div>

                <div className="flex flex-wrap items-center gap-3">
                  <Switch
                    size="sm"
                    isSelected={showHtmlTags}
                    onChange={setShowHtmlTags}
                    aria-label="显示 HTML 标签"
                  >
                    <Switch.Content className="text-xs">
                      <Switch.Control>
                        <Switch.Thumb />
                      </Switch.Control>
                      HTML 标签
                    </Switch.Content>
                  </Switch>

                  <Button
                    size="sm"
                    variant="tertiary"
                    onPress={() => {
                      if (exam) {
                        void invoke("open_uooc_task_portal", { taskId: exam.taskId });
                      }
                    }}
                    className="flex items-center gap-1 text-xs"
                  >
                    <LuExternalLink aria-hidden="true" />
                    官网处理附件
                  </Button>

                  {secondsRemaining !== null && (
                    <Chip
                      variant="soft"
                      color={secondsRemaining < 300 ? "danger" : "default"}
                      className="flex items-center gap-1 font-mono text-sm"
                    >
                      <LuClock />
                      剩余时间: {formatTime(secondsRemaining)}
                    </Chip>
                  )}

                  {lastSavedTime && (
                    <span className="text-xs text-muted">草稿已于 {lastSavedTime} 暂存</span>
                  )}

                  <Chip variant="tertiary" className="text-xs">
                    已作答 {answeredCount} / {questions.length} 题
                  </Chip>
                </div>
              </Modal.Header>

              <Modal.Body className="flex-1 overflow-y-auto px-6 py-4">
                {loading && (
                  <div className="flex h-64 flex-col items-center justify-center gap-3">
                    <Spinner size="lg" />
                    <p className="text-sm text-muted">正在加载试题题目与题库匹配...</p>
                  </div>
                )}

                {error && (
                  <Alert status="danger">
                    <Alert.Indicator />
                    <Alert.Content>
                      <Alert.Title>试卷加载失败</Alert.Title>
                      <Alert.Description>{error}</Alert.Description>
                    </Alert.Content>
                  </Alert>
                )}

                {paper && (
                  <div className="space-y-6">
                    {paper.isSubmitted && !manual && (
                      <Alert status="accent" className="flex items-center justify-between">
                        <Alert.Indicator />
                        <Alert.Content>
                          <Alert.Title>当前试卷已提交记录</Alert.Title>
                          <Alert.Description>
                            以下展示您已提交的答卷与标准解析。如需修改重答，可点击“自主答题/修改”。
                          </Alert.Description>
                        </Alert.Content>
                        <Button size="sm" variant="secondary" onPress={() => setManual(true)}>
                          自主答题/修改
                        </Button>
                      </Alert>
                    )}

                    {questions.map((q: QuizQuestion, index: number) => {
                      const currentValues = answers[q.id] || [];
                      const qReviews = (paper.reviews || []).filter((r) => r.questionId === q.id);

                      // 填空题提取匹配标准答案
                      const stdBlankReview = qReviews.find(
                        (r) =>
                          (r.message.includes("题库") ||
                            r.message.includes("标准答案") ||
                            r.message.includes("已确认答案")) &&
                          !r.message.includes("排除"),
                      );
                      const aiBlankReview = qReviews.find(
                        (r) =>
                          (r.message.includes("AI") || r.message.includes("判定")) &&
                          !r.message.includes("题库") &&
                          r.answers.length > 0,
                      );

                      return (
                        <Card key={q.id} className="border border-border/60">
                          <Card.Header className="flex flex-wrap items-center justify-between gap-2 pb-2">
                            <div className="flex items-center gap-2">
                              <span className="font-mono font-bold text-accent">第 {index + 1} 题</span>
                              <Chip size="sm" variant="secondary">
                                {q.kind === "single"
                                  ? "单选题"
                                  : q.kind === "multiple"
                                    ? "多选题"
                                    : q.kind === "subjective"
                                      ? "主观题"
                                      : "填空题"}
                              </Chip>
                            </div>

                            {qReviews.map((r, rIdx) => (
                              <Chip
                                key={rIdx}
                                size="sm"
                                variant="soft"
                                color="success"
                                className="flex items-center gap-1 text-xs"
                              >
                                <LuDatabase className="h-3 w-3" />
                                {r.message}
                              </Chip>
                            ))}
                          </Card.Header>

                          <Card.Content className="space-y-4 pt-1">
                            <div className="text-base font-medium leading-relaxed">
                              <DimmedHtmlText text={q.text} />
                            </div>

                            {/附件|下载|上传|文件|\.pdf|\.docx?|\.zip|\.rar|bosSync/i.test(q.text) && (
                              <div className="flex items-center justify-between gap-2 rounded-lg bg-surface-secondary/40 p-2 text-xs text-muted">
                                <span>提示：此题可能涉及附件查看或文件上传</span>
                                <Button
                                  size="sm"
                                  variant="tertiary"
                                  onPress={() => {
                                    if (exam) {
                                      void invoke("open_uooc_task_portal", { taskId: exam.taskId });
                                    }
                                  }}
                                  className="text-xs"
                                >
                                  <LuExternalLink aria-hidden="true" />
                                  在官网打开
                                </Button>
                              </div>
                            )}

                            {q.images && q.images.length > 0 && (
                              <div className="flex flex-wrap gap-2">
                                {q.images.map((imgSrc, imgIdx) => (
                                  <img
                                    key={imgIdx}
                                    src={imgSrc}
                                    alt={`试题插图 ${imgIdx + 1}`}
                                    className="max-h-60 rounded border object-contain"
                                  />
                                ))}
                              </div>
                            )}

                            {/* 单选题 / 判断题选项交互（含选项边框高亮与匹配项提示 Chip） */}
                            {q.kind === "single" && (
                              <RadioGroup
                                aria-label={`第 ${index + 1} 题答案`}
                                value={currentValues[0] || ""}
                                onChange={(val) => {
                                  if (!canEdit) return;
                                  setAnswers((prev) => ({
                                    ...prev,
                                    [q.id]: val ? [val] : [],
                                  }));
                                }}
                                isDisabled={!canEdit}
                                className="grid gap-2"
                              >
                                {q.options.map((opt) => {
                                  const badges = getOptionBadges(q, opt.id, opt.text, currentValues);
                                  const isRadioChecked = currentValues[0] === opt.id;
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
                                      key={opt.id}
                                      value={opt.id}
                                      className={`flex items-start gap-2.5 rounded-lg border p-2.5 transition-colors cursor-pointer ${borderStyle}`}
                                    >
                                      <Radio.Content className="flex items-start gap-2 w-full">
                                        <Radio.Control className="mt-0.5">
                                          <Radio.Indicator />
                                        </Radio.Control>
                                        <span
                                          aria-hidden="true"
                                          className="font-bold text-foreground shrink-0 text-xs"
                                        >
                                          {opt.id.toUpperCase()}.
                                        </span>
                                        <span className="flex-1 text-xs text-foreground/90">
                                          <DimmedHtmlText text={opt.text} />
                                        </span>

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
                                      </Radio.Content>
                                    </Radio>
                                  );
                                })}
                              </RadioGroup>
                            )}

                            {/* 多选题选项交互（含选项边框高亮与匹配项提示 Chip） */}
                            {q.kind === "multiple" && (
                              <CheckboxGroup
                                aria-label={`第 ${index + 1} 题答案`}
                                value={currentValues}
                                onChange={(vals) => {
                                  if (!canEdit) return;
                                  setAnswers((prev) => ({
                                    ...prev,
                                    [q.id]: vals,
                                  }));
                                }}
                                isDisabled={!canEdit}
                                className="grid gap-2"
                              >
                                {q.options.map((opt) => {
                                  const badges = getOptionBadges(q, opt.id, opt.text, currentValues);
                                  const isBoxChecked = currentValues.includes(opt.id);
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
                                      key={opt.id}
                                      value={opt.id}
                                      className={`flex items-start gap-2.5 rounded-lg border p-2.5 transition-colors cursor-pointer ${borderStyle}`}
                                    >
                                      <Checkbox.Control className="mt-0.5">
                                        <Checkbox.Indicator />
                                      </Checkbox.Control>
                                      <Checkbox.Content className="flex items-start gap-2 w-full">
                                        <span
                                          aria-hidden="true"
                                          className="font-bold text-foreground shrink-0 text-xs"
                                        >
                                          {opt.id.toUpperCase()}.
                                        </span>
                                        <span className="flex-1 text-xs text-foreground/90">
                                          <DimmedHtmlText text={opt.text} />
                                        </span>

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
                                      </Checkbox.Content>
                                    </Checkbox>
                                  );
                                })}
                              </CheckboxGroup>
                            )}

                            {/* 填空题交互与匹配提示 */}
                            {q.kind === "text" && (
                              <div className="space-y-3">
                                {Array.from({ length: Math.max(1, q.blanks || 1) }).map((_, bIdx) => {
                                  const stdVal = stdBlankReview?.answers[bIdx];
                                  const aiVal = aiBlankReview?.answers[bIdx];

                                  return (
                                    <TextField
                                      key={bIdx}
                                      value={currentValues[bIdx] || ""}
                                      onChange={(val) => {
                                        if (!canEdit) return;
                                        setAnswers((prev) => {
                                          const arr = [...(prev[q.id] || [])];
                                          arr[bIdx] = val;
                                          return { ...prev, [q.id]: arr };
                                        });
                                      }}
                                      isDisabled={!canEdit}
                                    >
                                      <Label className="flex items-center justify-between text-xs font-medium w-full">
                                        <span>
                                          第 {index + 1} 题{q.blanks > 1 ? ` · 第 ${bIdx + 1} 空` : "填空作答"}
                                        </span>
                                        <div className="flex items-center gap-1.5 shrink-0">
                                          {stdVal && (
                                            <Chip size="sm" color="success">
                                              标准答案: {stripHtml(stdVal)}
                                            </Chip>
                                          )}
                                          {!stdVal && aiVal && (
                                            <Chip size="sm" color="accent">
                                              AI判定: {stripHtml(aiVal)}
                                            </Chip>
                                          )}
                                        </div>
                                      </Label>
                                      <TextArea rows={2} className="text-xs" />
                                    </TextField>
                                  );
                                })}
                              </div>
                            )}

                            {/* 主观题作答与 AI 解答 */}
                            {q.kind === "subjective" && (
                              <div className="space-y-3">
                                <div className="flex items-center justify-between">
                                  <Label className="text-xs font-medium">主观题/问答题作答</Label>
                                  {canEdit && (
                                    <Button
                                      size="sm"
                                      variant="secondary"
                                      isDisabled={generatingAiQid === q.id}
                                      onPress={() => void handleGenerateSubjective(q)}
                                      className="flex items-center gap-1 text-xs"
                                    >
                                      {generatingAiQid === q.id ? (
                                        <Spinner size="sm" aria-label="正在生成" />
                                      ) : (
                                        <LuSparkles className="text-accent" aria-hidden="true" />
                                      )}
                                      AI 解答
                                    </Button>
                                  )}
                                </div>
                                <TextArea
                                  value={currentValues[0] || ""}
                                  onChange={(e) => {
                                    if (!canEdit) return;
                                    const val = e.target.value;
                                    setAnswers((prev) => ({
                                      ...prev,
                                      [q.id]: [val],
                                    }));
                                  }}
                                  disabled={!canEdit}
                                  rows={5}
                                  placeholder="在此输入主观题答案，或点击上方“AI 解答”按钮快速生成作答草稿…"
                                  className="text-sm font-normal"
                                />
                              </div>
                            )}
                          </Card.Content>
                        </Card>
                      );
                    })}
                  </div>
                )}
              </Modal.Body>

              <Modal.Footer className="flex flex-wrap items-center justify-between border-t bg-surface/50 p-4">
                <div className="flex items-center gap-2">
                  <Button variant="tertiary" onPress={onClose}>
                    返回列表
                  </Button>
                  {paper?.isSubmitted && !manual && (
                    <Button variant="secondary" size="sm" onPress={() => setManual(true)}>
                      自主答题/修改
                    </Button>
                  )}
                </div>

                {paper && canEdit && (
                  <div className="flex flex-wrap items-center gap-2">
                    <Button
                      variant="secondary"
                      size="sm"
                      isDisabled={matchingQa || solvingAi}
                      onPress={() => void handleMatchQa()}
                      className="text-xs"
                    >
                      {matchingQa ? <Spinner size="sm" /> : <LuDatabase className="text-accent" />}
                      匹配题库
                    </Button>

                    <Button
                      variant="secondary"
                      size="sm"
                      isDisabled={matchingQa || solvingAi}
                      onPress={() => void handleSolveAi()}
                      className="text-xs"
                    >
                      {solvingAi ? <Spinner size="sm" /> : <LuSparkles className="text-accent" />}
                      AI 答题
                    </Button>

                    <Button
                      variant="secondary"
                      size="sm"
                      isDisabled={saving || submitting}
                      onPress={() => void handleSaveDraft()}
                      className="text-xs"
                    >
                      {saving ? <Spinner size="sm" /> : <LuSave />}
                      保存草稿
                    </Button>

                    <Button
                      variant="primary"
                      size="sm"
                      isDisabled={saving || submitting}
                      onPress={() => setConfirmSubmitOpen(true)}
                      className="text-xs"
                    >
                      {submitting ? <Spinner size="sm" /> : <LuSend />}
                      提交试卷
                    </Button>
                  </div>
                )}
              </Modal.Footer>
            </Modal.Dialog>
          </Modal.Container>
        </Modal.Backdrop>
      </Modal>

      <AlertDialog isOpen={confirmSubmitOpen} onOpenChange={setConfirmSubmitOpen}>
        <AlertDialog.Backdrop>
          <AlertDialog.Container size="sm">
            <AlertDialog.Dialog className="max-w-md">
              <AlertDialog.Icon status="danger" />
              <AlertDialog.Header>
                <AlertDialog.Heading>确认提交试卷？</AlertDialog.Heading>
              </AlertDialog.Header>
              <AlertDialog.Body className="space-y-3">
                <p className="text-sm font-medium">提交后将记录最终成绩并不可撤销。</p>
                {unansweredIndices.length > 0 ? (
                  <Alert status="danger">
                    <Alert.Indicator />
                    <Alert.Content>
                      <Alert.Title>仍有未作答题目！</Alert.Title>
                      <Alert.Description>
                        您还有 {unansweredIndices.length} 道题目未填写答案（第{" "}
                        {unansweredIndices.slice(0, 5).join("、")}
                        {unansweredIndices.length > 5 ? "..." : ""} 题）。
                      </Alert.Description>
                    </Alert.Content>
                  </Alert>
                ) : (
                  <Alert status="accent">
                    <Alert.Indicator />
                    <Alert.Content>
                      <Alert.Title>所有题目已完成作答</Alert.Title>
                      <Alert.Description>
                        共 {questions.length} 道题目均已填写答案，准备好后点击“确认提交”。
                      </Alert.Description>
                    </Alert.Content>
                  </Alert>
                )}
              </AlertDialog.Body>
              <AlertDialog.Footer className="flex justify-end gap-2">
                <AlertDialog.CloseTrigger className="dialog-cancel" aria-label="检查题目" />
                <Button
                  variant="danger"
                  isDisabled={submitting}
                  onPress={() => void handleConfirmSubmit()}
                >
                  {submitting ? <Spinner size="sm" /> : <LuSend />}
                  确认提交
                </Button>
              </AlertDialog.Footer>
            </AlertDialog.Dialog>
          </AlertDialog.Container>
        </AlertDialog.Backdrop>
      </AlertDialog>
    </>
  );
}
