import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Button,
  Checkbox,
  CheckboxGroup,
  Chip,
  Label,
  Radio,
  RadioGroup,
  Spinner,
  TextArea,
  TextField,
  toast,
} from "@heroui/react";
import type { QuizAnswers, QuizState, UoocSnapshot } from "./types";

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
  useEffect(() => {
    setAnswers(quiz.answers);
  }, [quiz.revision]);
  const running =
    active && ["preparing", "generating", "submitting", "verifying"].includes(quiz.stage);
  const disabled = busy || running || !(typeof weight === "number" && weight > 0);
  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    try {
      await fn();
    } catch (e) {
      const msg = typeof e === "string" ? e : "测验操作失败，请重试";
      toast.danger("测验操作失败", { description: msg });
    } finally {
      setBusy(false);
    }
  };
  const control = async (action: "generate" | "check") => {
    onSnapshot(
      await invoke<UoocSnapshot>("control_uooc_quiz", {
        cid: quiz.cid,
        revision: quiz.revision,
        action,
      }),
    );
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
  return (
    <section className="grid gap-3" aria-label="课程测验">
      <div className="flex flex-wrap items-center gap-2">
        <h3 className="font-semibold">{quiz.title || "课程测验"}</h3>
        {quiz.stage === "passed" && <Chip color="success">测验已完成</Chip>}
        {running && <Spinner size="sm" aria-label="测验处理中" />}
      </div>
      <p role="status" className="text-sm text-muted">
        {quiz.message || "视频完成或遇到闯关测验时开始处理"}
      </p>
      {(quiz.stage !== "awaitingChoice" || manual) &&
        quiz.questions.map((q, i) => (
          <div key={q.id} className="grid gap-2">
            <p className="whitespace-pre-wrap">
              {i + 1}. {q.text}
            </p>
            {q.images.map((src, index) => (
              <img
                key={index}
                src={src}
                alt={`第 ${i + 1} 题图片 ${index + 1}`}
                className="max-h-96 max-w-full object-contain"
              />
            ))}
            {q.kind === "single" ? (
              <RadioGroup
                aria-label={`第 ${i + 1} 题答案`}
                value={answers[q.id]?.[0] ?? ""}
                onChange={(v) => set(q.id, [v])}
                isDisabled={disabled}
              >
                {q.options.map((o) => (
                  <Radio key={o.id} value={o.id}>
                    <Radio.Content>
                      <Radio.Control>
                        <Radio.Indicator />
                      </Radio.Control>
                      {o.text}
                    </Radio.Content>
                  </Radio>
                ))}
              </RadioGroup>
            ) : q.kind === "multiple" ? (
              <CheckboxGroup
                aria-label={`第 ${i + 1} 题答案`}
                value={answers[q.id] ?? []}
                onChange={(v) => set(q.id, v)}
                isDisabled={disabled}
              >
                {q.options.map((o) => (
                  <Checkbox value={o.id} key={o.id}>
                    <Checkbox.Control>
                      <Checkbox.Indicator />
                    </Checkbox.Control>
                    <Checkbox.Content>{o.text}</Checkbox.Content>
                  </Checkbox>
                ))}
              </CheckboxGroup>
            ) : (
              Array.from({ length: q.blanks }, (_, blank) => (
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
                  <Label>
                    第 {i + 1} 题{q.blanks > 1 ? ` · 第 ${blank + 1} 空` : "答案"}
                  </Label>
                  <TextArea rows={3} />
                </TextField>
              ))
            )}
            {quiz.reviews
              .filter((r) => r.questionId === q.id)
              .map((review, j) => (
                <div key={j} className="text-xs text-muted">
                  <p>{review.message}</p>
                  {review.answers.length > 0 && (
                    <p>
                      Jev 建议：
                      {review.answers
                        .map((id) => q.options.find((o) => o.id === id)?.text ?? id)
                        .join("；")}
                    </p>
                  )}
                  <p>
                    {Object.entries(review.probabilities)
                      .map(
                        ([id, p]) =>
                          `${q.options.find((o) => o.id === id)?.text ?? id}：${(p * 100).toFixed(1)}%`,
                      )
                      .join(" · ")}
                  </p>
                </div>
              ))}
          </div>
        ))}
      {quiz.taskId && (
        <div className="flex flex-wrap gap-2">
          {quiz.stage === "awaitingChoice" && (
            <Button variant="secondary" isDisabled={disabled} onPress={() => setManual(true)}>
              自主答题
            </Button>
          )}
          <Button
            variant="secondary"
            isDisabled={disabled || quiz.questions.length === 0}
            onPress={() => void run(() => control("generate"))}
          >
            AI 答题
          </Button>
          <Button
            variant="secondary"
            isDisabled={disabled || quiz.questions.length === 0}
            onPress={() =>
              void run(async () => {
                await save();
              })
            }
          >
            保存答案草稿
          </Button>
          <Button
            isDisabled={disabled || dryRun || quiz.questions.length === 0}
            onPress={() =>
              void run(async () => {
                const saved = await save();
                if (!saved) throw new Error("草稿已失效");
                onSnapshot(
                  await invoke<UoocSnapshot>("control_uooc_quiz", {
                    cid: quiz.cid,
                    revision: saved.revision,
                    action: "submit",
                  }),
                );
              })
            }
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
            onPress={() => void run(() => control("check"))}
          >
            我已在官网处理，检查并继续
          </Button>
        </div>
      )}
      {dryRun && <p className="text-xs text-muted">预览模式：草稿不会提交到官网。</p>}
    </section>
  );
}
