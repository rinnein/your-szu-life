import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Button,
  Checkbox,
  CheckboxGroup,
  Chip,
  Label,
  Modal,
  ProgressBar,
  Radio,
  RadioGroup,
  Spinner,
  TextArea,
  TextField,
  toast,
} from "@heroui/react";
import {
  LuBot,
  LuCircleCheck,
  LuDatabase,
  LuSparkles,
} from "react-icons/lu";
import { DimmedHtmlText } from "./DimmedHtmlText";
import type { AiSessionEntry, QuizAnswers, QuizQuestion, QuizState, UoocSnapshot } from "./types";

export function QuizPanel({
  quiz,
  dryRun,
  weight,
  onSnapshot,
  active,
}: {
  quiz: QuizState;
  dryRun: boolean;
  weight?: number | null;
  onSnapshot: (v: UoocSnapshot) => void;
  active: boolean;
}) {
  const [answers, setAnswers] = useState<QuizAnswers>(quiz.answers);
  const [busy, setBusy] = useState(false);
  const [manual, setManual] = useState(false);
  const [actionModal, setActionModal] = useState<{
    type: "matchQa" | "generate" | "submit";
    title: string;
    step: number;
    completed: boolean;
    summary?: string;
  } | null>(null);

  const [liveSessions, setLiveSessions] = useState<AiSessionEntry[]>(quiz.aiSessions ?? []);

  useEffect(() => {
    if (quiz.aiSessions && quiz.aiSessions.length > 0) {
      setLiveSessions(quiz.aiSessions);
    }
  }, [quiz.aiSessions]);

  useEffect(() => {
    let active = true;
    const unlistenPromise = listen<AiSessionEntry>("uooc-ai-session-progress", (event) => {
      if (!active) return;
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
    });
    return () => {
      active = false;
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, []);

  // 监听 quiz 变化，自动将答案值规范化为对应选项的 option.id；若未作答但有题库或 AI 推荐答案，自动完成选中
  useEffect(() => {
    const nextAnswers: QuizAnswers = { ...quiz.answers };
    for (const q of quiz.questions) {
      const curr = nextAnswers[q.id];
      if (curr && curr.length > 0) {
        const mapped = curr.map((val) => {
          const found = q.options.find(
            (o) =>
              o.id === val ||
              o.id.toLowerCase() === val.toLowerCase() ||
              o.text.trim() === val.trim(),
          );
          return found ? found.id : val;
        });
        nextAnswers[q.id] = mapped;
      } else {
        // 自动选择 1：优先自动选中题库匹配的标准答案
        const stdReview = quiz.reviews.find(
          (r) =>
            r.questionId === q.id &&
            (r.message.includes("题库") || r.message.includes("标准答案") || r.message.includes("已确认答案")) &&
            !r.message.includes("排除"),
        );
        if (stdReview && stdReview.answers.length > 0) {
          const stdMapped = stdReview.answers.map((ans) => {
            const found = q.options.find(
              (o) =>
                o.id === ans ||
                o.id.toLowerCase() === ans.toLowerCase() ||
                o.text.trim() === ans.trim(),
            );
            return found ? found.id : ans;
          });
          nextAnswers[q.id] = stdMapped;
        } else {
          // 自动选择 2：自动选中 AI 判定的推荐答案
          const aiRev = quiz.reviews.find(
            (r) =>
              r.questionId === q.id &&
              r.answers.length > 0 &&
              !r.message.includes("排除"),
          );
          if (aiRev && aiRev.answers.length > 0) {
            const aiMapped = aiRev.answers.map((ans) => {
              const found = q.options.find(
                (o) =>
                  o.id === ans ||
                  o.id.toLowerCase() === ans.toLowerCase() ||
                  o.text.trim() === ans.trim(),
              );
              return found ? found.id : ans;
            });
            nextAnswers[q.id] = aiMapped;
          }
        }
      }
    }
    setAnswers(nextAnswers);
  }, [quiz.revision, quiz.answers, quiz.reviews, quiz.questions]);

  const running =
    active && ["preparing", "generating", "submitting", "verifying"].includes(quiz.stage);
  const disabled = busy || running || !(typeof weight === "number" && weight > 0);

  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    try {
      await fn();
    } catch (e) {
      const msg = typeof e === "string" ? e : (e as { message?: string })?.message || "测验操作失败，请重试";
      toast.danger("测验操作失败", { description: msg });
      setActionModal(null);
    } finally {
      setBusy(false);
    }
  };

  const control = async (action: "generate" | "check" | "matchQa") => {
    const nextSnapshot = await invoke<UoocSnapshot>("control_uooc_quiz", {
      cid: quiz.cid,
      revision: quiz.revision,
      action,
    });
    onSnapshot(nextSnapshot);
    const nextQuiz = Object.values(nextSnapshot.quizzes ?? {}).find(
      (q) => q.accountId === quiz.accountId && q.cid === quiz.cid,
    );
    if (nextQuiz) {
      setAnswers(nextQuiz.answers);
      if (nextQuiz.aiSessions && nextQuiz.aiSessions.length > 0) {
        setLiveSessions(nextQuiz.aiSessions);
      }
    }
    return nextSnapshot;
  };

  const save = async () => {
    const state = await invoke<UoocSnapshot>("save_uooc_quiz_answers", {
      cid: quiz.cid,
      revision: quiz.revision,
      answers,
    });
    onSnapshot(state);
    return Object.values(state.quizzes ?? {}).find(
      (q) => q.accountId === quiz.accountId && q.cid === quiz.cid,
    );
  };

  const set = (id: string, values: string[]) => setAnswers((old) => ({ ...old, [id]: values }));

  const handleActionWithModal = async (
    action: "matchQa" | "generate" | "submit",
    title: string,
  ) => {
    if (action === "generate") {
      setLiveSessions([]);
    }
    setActionModal({ type: action, title, step: 2, completed: false });
    await run(async () => {
      let nextSnapshot: UoocSnapshot | undefined;
      if (action === "submit") {
        const saved = await save();
        if (!saved) throw new Error("草稿已失效");
        nextSnapshot = await invoke<UoocSnapshot>("control_uooc_quiz", {
          cid: quiz.cid,
          revision: saved.revision,
          action: "submit",
        });
        onSnapshot(nextSnapshot);
      } else {
        nextSnapshot = await control(action);
      }
      const updatedQuiz = nextSnapshot
        ? Object.values(nextSnapshot.quizzes ?? {}).find(
            (q) => q.accountId === quiz.accountId && q.cid === quiz.cid,
          )
        : undefined;
      const summaryMsg = updatedQuiz?.message;
      setActionModal((prev) => (prev ? { ...prev, step: 3, completed: true, summary: summaryMsg } : null));
    });
  };

  // 判定并格式化题目类型标签（参考本地题库浏览模块）
  const resolveQuestionTypeLabel = (q: QuizQuestion): string => {
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
    const optIdLower = optId.toLowerCase();
    const optTextTrim = optText.trim();

    // 1. 题库匹配的标准答案 (来自题库 review 或已通过状态的官方确认答案)
    const isPassed = quiz.stage === "passed";
    const passedAnswers = isPassed ? (quiz.answers[q.id] ?? []) : [];
    const passedMatch = passedAnswers.some(
      (ans) => ans.toLowerCase() === optIdLower || ans.trim() === optTextTrim,
    );

    const bankReview = quiz.reviews.find(
      (r) =>
        r.questionId === q.id &&
        (r.message.includes("题库") || r.message.includes("已确认答案") || r.message.includes("标准答案")) &&
        !r.message.includes("排除"),
    );
    const bankMatch = bankReview
      ? bankReview.answers.some(
          (ans) => ans.toLowerCase() === optIdLower || ans.trim() === optTextTrim,
        )
      : false;

    const isStandard = passedMatch || bankMatch;

    // 2. 题库排除答案 (来自题库排除 review)
    const excludeReview = quiz.reviews.find(
      (r) =>
        r.questionId === q.id &&
        (r.message.includes("排除") || r.message.includes("请勿选择")),
    );
    const isExcluded =
      !isStandard &&
      (excludeReview
        ? excludeReview.answers.some(
            (ans) => ans.toLowerCase() === optIdLower || ans.trim() === optTextTrim,
          ) ||
          excludeReview.message.includes(optId.toUpperCase()) ||
          (optTextTrim.length >= 2 && excludeReview.message.includes(optTextTrim))
        : false);

    // 3. AI 判定答案 (来自 AI review 推荐选项，非题库标准答案)
    const aiReview = quiz.reviews.find(
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
      (aiReview
        ? aiReview.answers.some(
            (ans) => ans.toLowerCase() === optIdLower || ans.trim() === optTextTrim,
          )
        : false);

    // 4. 用户当前是否在界面上选择/勾选了该项（支持自动选择标准答案与 AI 判定答案）
    const isSelected =
      currentSelected.some(
        (ans) => ans.toLowerCase() === optIdLower || ans.trim() === optTextTrim,
      ) ||
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
      const matched = q.options.find(
        (o) =>
          o.id === current ||
          o.id.toLowerCase() === current.toLowerCase() ||
          o.text.trim() === current.trim(),
      );
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
        const matched = q.options.find(
          (o) =>
            o.id === val ||
            o.id.toLowerCase() === val.toLowerCase() ||
            o.text.trim() === val.trim(),
        );
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

  return (
    <>
      {/* 4. 上面的 课程测验 <label>测验已完成</> <button>查看测验</> 等作为 Modal Header */}
      <Modal.Header className="shrink-0 border-b border-separator/40 p-4 sm:p-5">
        <div className="flex flex-wrap items-center justify-between gap-3 pr-8">
          <div className="flex flex-wrap items-center gap-2">
            <Modal.Heading className="text-base font-semibold">
              {quiz.title || "课程测验"}
            </Modal.Heading>
            {quiz.stage === "passed" && <Chip color="success">测验已完成</Chip>}
            {running && <Spinner size="sm" aria-label="测验处理中" />}
          </div>
          {(quiz.stage === "passed" || quiz.message.includes("当前测验已由服务端确认完成")) && (
            <Button
              variant="secondary"
              size="sm"
              isDisabled={busy || running}
              onPress={() =>
                void run(async () => {
                  const next = await invoke<UoocSnapshot>("fetch_uooc_completed_quiz", {
                    cid: quiz.cid,
                    taskId: quiz.taskId || undefined,
                    resourceId: quiz.resourceId || undefined,
                  });
                  onSnapshot(next);
                  toast.success("已成功拉取当前测验已完成的题目与答案");
                })
              }
            >
              查看测验
            </Button>
          )}
        </div>
        <p role="status" className="text-xs text-muted mt-1.5">
          {quiz.message || "视频完成或遇到闯关测验时开始处理"}
        </p>
      </Modal.Header>

      {/* 1. 题目与选项展示模块：仅 Body 随题目滚动；标注器显示在选项旁边，下方不再堆砌答案框 */}
      <Modal.Body className="flex-1 overflow-y-auto p-4 sm:p-6 min-h-0 space-y-4">
        {(quiz.stage !== "awaitingChoice" || manual) &&
          quiz.questions.map((q, i) => {
            const typeLabel = resolveQuestionTypeLabel(q);

            // 针对填空题查找标准答案与AI判定
            const isPassed = quiz.stage === "passed";
            const passedAnswers = isPassed ? (quiz.answers[q.id] ?? []) : [];
            const bankReview = quiz.reviews.find(
              (r) =>
                r.questionId === q.id &&
                (r.message.includes("题库") || r.message.includes("已确认答案") || r.message.includes("标准答案")),
            );
            const aiReview = quiz.reviews.find(
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
                    <span className="font-semibold text-sm text-foreground">
                      第 {i + 1} 题
                    </span>
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
                  <RadioGroup
                    aria-label={`第 ${i + 1} 题答案`}
                    value={getSelectedRadioValue(q)}
                    onChange={(v) => set(q.id, [v])}
                    isDisabled={disabled}
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
                              {o.id.toUpperCase()}.
                            </span>
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
                          </Radio.Content>
                        </Radio>
                      );
                    })}
                  </RadioGroup>
                ) : q.kind === "multiple" ? (
                  <CheckboxGroup
                    aria-label={`第 ${i + 1} 题答案`}
                    value={getSelectedCheckboxValues(q)}
                    onChange={(v) => set(q.id, v)}
                    isDisabled={disabled}
                    className="grid gap-2"
                  >
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
                              {o.id.toUpperCase()}.
                            </span>
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
                          </Checkbox.Content>
                        </Checkbox>
                      );
                    })}
                  </CheckboxGroup>
                ) : (
                  Array.from({ length: q.blanks }, (_, blank) => {
                    const stdText = passedAnswers[blank] || bankReview?.answers[blank];
                    const aiText = aiReview?.answers[blank];

                    return (
                      <TextField
                        key={blank}
                        value={answers[q.id]?.[blank] ?? ""}
                        onChange={(v) => {
                          const values = Array.from(
                            { length: q.blanks },
                            (_, j) => answers[q.id]?.[j] ?? "",
                          );
                          values[blank] = v;
                          set(q.id, values);
                        }}
                        isDisabled={disabled}
                      >
                        <Label className="flex items-center justify-between text-xs font-medium w-full">
                          <span>
                            第 {i + 1} 题{q.blanks > 1 ? ` · 第 ${blank + 1} 空` : "答案"}
                          </span>
                          <div className="flex items-center gap-1.5 shrink-0">
                            {stdText && (
                              <Chip size="sm" color="success">
                                标准答案: {stdText}
                              </Chip>
                            )}
                            {!stdText && aiText && (
                              <Chip size="sm" color="accent">
                                AI判定: {aiText}
                              </Chip>
                            )}
                          </div>
                        </Label>
                        <TextArea rows={2} className="text-xs" />
                      </TextField>
                    );
                  })
                )}

                {/* AI / Jev 附加概率分布说明（如果有） */}
                {quiz.reviews
                  .filter(
                    (r) =>
                      r.questionId === q.id &&
                      (r.message.includes("Jev") || Object.keys(r.probabilities).length > 0),
                  )
                  .map((review, j) => (
                    <div
                      key={j}
                      className="grid gap-1 rounded-lg border border-accent/25 bg-accent/5 p-2.5 text-xs text-muted"
                    >
                      <p className="font-medium text-foreground">{review.message}</p>
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
      </Modal.Body>

      {/* 3. 操作按钮作为 Modal Footer，固定在底部不随题目滚动 */}
      {quiz.taskId && (
        <Modal.Footer className="shrink-0 border-t border-separator/40 p-3 sm:p-4 flex flex-wrap items-center justify-end gap-2 bg-surface/90 backdrop-blur">
          {quiz.stage === "awaitingChoice" && (
            <Button variant="secondary" isDisabled={disabled} onPress={() => setManual(true)}>
              自主答题
            </Button>
          )}
          <Button
            variant="secondary"
            isDisabled={disabled || quiz.questions.length === 0}
            onPress={() => void handleActionWithModal("matchQa", "题库匹配流程")}
          >
            匹配题库
          </Button>
          <Button
            variant="secondary"
            isDisabled={disabled || quiz.questions.length === 0}
            onPress={() => void handleActionWithModal("generate", "AI 智能答题流程")}
          >
            AI 答题
          </Button>
          <Button
            variant="secondary"
            isDisabled={disabled || quiz.questions.length === 0}
            onPress={() =>
              void run(async () => {
                await save();
                toast.success("草稿已保存");
              })
            }
          >
            保存答案草稿
          </Button>
          <Button
            isDisabled={disabled || dryRun || quiz.questions.length === 0}
            onPress={() => void handleActionWithModal("submit", "测验提交与确认流程")}
          >
            确认答案并提交
          </Button>
          <Button
            variant="tertiary"
            isDisabled={disabled}
            onPress={() =>
              void run(async () => {
                await invoke("open_uooc_quiz_portal", { cid: quiz.cid });
              })
            }
          >
            打开官方试卷
          </Button>
          <Button
            variant="tertiary"
            isDisabled={busy || running}
            onPress={() => void run(async () => { await control("check"); })}
          >
            我已在官网处理，检查并继续
          </Button>
        </Modal.Footer>
      )}

      {dryRun && <p className="text-xs text-muted px-4 pb-2">预览模式：草稿不会提交到官网。</p>}

      {/* 2. 点击匹配题库、AI 答题等时弹出的运行流程与会话调用 Modal */}
      <Modal
        isOpen={Boolean(actionModal)}
        onOpenChange={(open) => {
          if (!open && !busy && !running) {
            setActionModal(null);
          }
        }}
      >
        <Modal.Backdrop>
          <Modal.Container size="cover" className="max-w-2xl">
            <Modal.Dialog aria-label={actionModal?.title || "测验运行流程"}>
              {isModalFinished && <Modal.CloseTrigger />}
              <Modal.Header>
                <div className="flex items-center gap-2.5">
                  {!isModalFinished ? (
                    <Spinner size="sm" aria-label="处理中" />
                  ) : (
                    <LuCircleCheck className="text-success text-xl shrink-0" aria-hidden="true" />
                  )}
                  <Modal.Heading>{actionModal?.title || "测验运行流程"}</Modal.Heading>
                </div>
              </Modal.Header>
              <Modal.Body className="grid gap-4 p-4 sm:p-6 text-sm max-h-[70vh] overflow-y-auto">
                <div className="rounded-xl border border-separator/50 bg-surface/40 p-4 grid gap-2.5">
                  <div className="flex items-center justify-between text-xs text-muted">
                    <span>当前执行状态</span>
                    <span className={`font-mono ${isModalFinished ? "text-success font-semibold" : "text-accent"}`}>
                      {isModalFinished ? "已完成" : "进行中"}
                    </span>
                  </div>
                  <p className="font-medium text-foreground">
                    {actionModal?.summary || quiz.message || (isModalFinished ? "流程已处理完成" : "正在处理测验任务...")}
                  </p>
                  {!isModalFinished && <ProgressBar isIndeterminate size="sm" color="accent" />}
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
                            {isModalFinished ? "答案草稿已提交至服务端判题确认" : "提交答案草稿至服务端判题确认"}
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
                        <LuBot className="text-accent text-sm" aria-hidden="true" /> AI 会话调用 Session
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
                                    <Spinner size="sm" aria-label="处理中" />
                                  ) : isBatchError ? (
                                    <span className="text-danger font-bold text-xs">✕</span>
                                  ) : (
                                    <span className="text-success font-bold text-xs">✓</span>
                                  )}
                                  <span className="font-semibold">
                                    批次 {session.batchIndex} (共 {session.questionIds?.length || 1} 题)
                                  </span>
                                </div>
                                <span className="text-[11px] font-mono text-accent shrink-0">
                                  {isBatchPending
                                    ? "正在请求模型..."
                                    : isBatchError
                                      ? "调用失败"
                                      : `已生成 ${Object.keys(session.answers || {}).length} 题答案`}
                                </span>
                              </summary>

                              <div className="mt-1 text-[11px] text-muted">
                                <span>包含题目：</span>
                                <span className="font-mono">
                                  {session.questionTitles?.join("、 ") || session.questionIds?.join(", ")}
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

                                {session.response ? (
                                  <div>
                                    <span className="font-semibold text-muted">
                                      AI 原始回复内容 (Response):
                                    </span>
                                    <pre className="mt-1 p-2 rounded bg-background/80 font-mono text-[10px] overflow-x-auto whitespace-pre-wrap text-accent font-medium max-h-36">
                                      {session.response}
                                    </pre>
                                  </div>
                                ) : isBatchPending ? (
                                  <div className="flex items-center gap-2 text-muted italic text-[11px] py-1">
                                    <Spinner size="sm" /> 正在等待模型返回作答内容...
                                  </div>
                                ) : null}

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
                        <Spinner size="sm" /> 正在准备向 AI 模型发送测验题目并等待会话响应...
                      </div>
                    )}
                  </div>
                )}
              </Modal.Body>
              <Modal.Footer>
                <Button
                  variant="primary"
                  isDisabled={!isModalFinished}
                  onPress={() => setActionModal(null)}
                >
                  {!isModalFinished ? "正在处理中..." : "查看试卷与作答结果"}
                </Button>
              </Modal.Footer>
            </Modal.Dialog>
          </Modal.Container>
        </Modal.Backdrop>
      </Modal>
    </>
  );
}
