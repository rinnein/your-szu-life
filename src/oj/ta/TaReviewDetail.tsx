import { useEffect, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { Button, Card, Dialog, LoadingIndicator, TextField } from "material/react";
import { AdaptiveTabs, Banner, Chip, Disclosure, NumberField, bindField, toast } from "../../ui";
import { errorMessage, useAsyncLoad } from "../../async-state";
import { usePending } from "../../use-pending";
import { Html } from "../OjPaper";
import { Verdict } from "../JudgeResult";
import {
  kindName,
  reviewTag,
  reviewTagColor,
  type ReviewDetail,
  type ReviewProblem,
  type ReviewScores,
  type SaveReviewResult,
  type Site,
  type WriteResult,
} from "../types";
import { invalidateTaReviews } from "./ta-cache";
import { TaCode } from "./TaCode";
import { TaSubmissionDetail } from "./TaSubmissionDetail";

const key = (problem: ReviewProblem) => `${problem.kind}:${problem.id}`;

export function TaReviewDetail({
  site,
  contestId,
  submissionId,
  desktop,
}: {
  site: Site;
  contestId: string;
  submissionId: string;
  desktop: boolean;
}) {
  const { state, reload, apply } = useAsyncLoad(() =>
    invoke<ReviewDetail>("get_oj_review_detail", {
      siteId: site.id,
      contestId,
      submissionId,
    }),
  );
  const [scores, setScores] = useState<Record<string, number>>({});
  const [comments, setComments] = useState<Record<string, string>>({});
  const [current, setCurrent] = useState<string>();
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [message, setMessage] = useState<string>();
  const [uncertain, setUncertain] = useState(false);
  const { isPending, run } = usePending();

  useEffect(() => {
    if (state.status !== "ready") return;
    const nextScores: Record<string, number> = {};
    const nextComments: Record<string, string> = {};
    for (const problem of state.data.problems) {
      nextScores[key(problem)] = problem.score;
      if (problem.comment) nextComments[key(problem)] = problem.comment;
    }
    setScores(nextScores);
    setComments(nextComments);
    setCurrent((old) =>
      old && state.data.problems.some((problem) => key(problem) === old)
        ? old
        : state.data.problems[0]
          ? key(state.data.problems[0])
          : undefined,
    );
  }, [state]);

  const detail =
    state.status === "ready"
      ? state.data
      : state.status === "loading" || state.status === "error"
        ? state.previous
        : undefined;
  const problem = detail?.problems.find((row) => key(row) === current);
  const items = useMemo(
    () =>
      detail?.problems.map((row) => ({
        value: key(row),
        label: row.displayLabel,
      })) ?? [],
    [detail],
  );
  const changed = useMemo(() => {
    if (!detail) return [];
    return detail.problems.filter((row) => (scores[key(row)] ?? row.score) !== row.score);
  }, [detail, scores]);
  const scoreOf = (row: ReviewProblem) => scores[key(row)] ?? row.score;
  const setScore = (row: ReviewProblem, value: number) =>
    setScores((old) => ({ ...old, [key(row)]: value }));

  const refresh = async () => {
    const ok = await reload();
    if (ok) {
      setUncertain(false);
      setMessage(undefined);
    }
  };

  const save = async () => {
    if (!detail) return;
    const payload: ReviewScores = {
      coding: {},
      codeFill: {},
      qa: {},
      choice: {},
      fillIn: {},
    };
    for (const row of detail.problems) {
      const score = scoreOf(row);
      const sid = row.submissionId ?? "-1";
      if (row.kind === 0) payload.coding[row.id] = { [sid]: score };
      else if (row.kind === 4) payload.codeFill[row.id] = { [sid]: score };
      else if (row.kind === 3) payload.qa[row.id] = { [sid]: score };
      else if (row.kind === 1) payload.choice[row.id] = score;
      else if (row.kind === 2) payload.fillIn[row.id] = score;
    }
    const outcome = await run("save", () =>
      invoke<SaveReviewResult>("save_oj_review", {
        input: {
          siteId: site.id,
          contestId,
          submissionId,
          scores: payload,
        },
      }),
    );
    if (!outcome.ok) {
      if (!outcome.skipped) setMessage(errorMessage(outcome.error));
      return;
    }
    const result = outcome.value;
    setConfirmOpen(false);
    if (!result.saved) {
      setMessage(result.message || "保存失败。");
      if (result.uncertain) setUncertain(true);
      return;
    }
    invalidateTaReviews(site.id, site.username, site.sessionGeneration, contestId);
    setUncertain(false);
    if (result.detail) apply({ ...result.detail, checked: true });
    toast.success("已保存并标记已阅。");
    if (result.message) {
      setMessage(result.message);
      void reload();
    }
  };

  const saveComment = async (row: ReviewProblem) => {
    if (!row.submissionId || row.submissionId === "-1") return;
    const outcome = await run(`comment:${key(row)}`, () =>
      invoke<WriteResult>("save_oj_review_comment", {
        input: {
          siteId: site.id,
          submissionId: row.submissionId,
          kind: row.kind,
          comment: comments[key(row)] ?? "",
        },
      }),
    );
    if (!outcome.ok) {
      if (!outcome.skipped) setMessage(errorMessage(outcome.error));
      return;
    }
    const result = outcome.value;
    if (result.saved) {
      toast.success("评语已保存。");
    } else {
      setMessage(result.message || "评语保存失败。");
    }
  };

  if (state.status === "idle" || (state.status === "loading" && !detail)) {
    return <LoadingIndicator size={24} aria-label="正在读取答卷" />;
  }
  if (state.status === "error" && !detail) {
    return (
      <Banner status="warning">
        <Banner.Title>答卷读取失败</Banner.Title>
        <Banner.Description>{state.message}</Banner.Description>
        <Button variant="tonal" onClick={() => void reload()}>
          重试
        </Button>
      </Banner>
    );
  }
  if (!detail || !problem) {
    return <p className="oj-muted">该答卷没有可批阅的题目。</p>;
  }

  const disabled = problem.submissionId === "-1";
  return (
    <section className="oj-ta-review">
      <Card variant="outlined" headline={`${detail.user.username} 的答卷`}>
        <div className="oj-actions">
          <Chip color={reviewTagColor(detail)}>{reviewTag(detail)}</Chip>
          <span className="oj-muted">
            总分 {detail.totalScore}
            {detail.user.name ? ` · ${detail.user.name}` : ""}
          </span>
          <Button
            variant="tonal"
            disabled={state.status === "loading"}
            onClick={() => void refresh()}
          >
            刷新
          </Button>
        </div>
      </Card>
      {(message || detail.warnings.length > 0 || uncertain) && (
        <Banner status={uncertain ? "danger" : "warning"}>
          {uncertain && (
            <Banner.Title>保存结果未知，请先刷新列表/答卷确认，不要重复提交。</Banner.Title>
          )}
          {message && <Banner.Description>{message}</Banner.Description>}
          {detail.warnings.length > 0 && (
            <Banner.Description>{detail.warnings.join("；")}</Banner.Description>
          )}
        </Banner>
      )}
      <AdaptiveTabs
        className="oj-ta-problem-tabs"
        items={items}
        value={current ?? items[0]?.value ?? ""}
        onChange={setCurrent}
        label="题目"
        ariaLabel="批阅题目切换"
      />
      <Card variant="outlined">
        <div className="oj-ta-review-problem">
          <div className="oj-actions">
            <h2>
              {problem.displayLabel}. {problem.title}
            </h2>
            <Chip>{kindName[problem.kind] ?? `题型 ${problem.kind}`}</Chip>
            <span className="oj-muted">满分 {problem.maxScore}</span>
          </div>
          {problem.loadError && (
            <Banner status="warning">
              <Banner.Description>{problem.loadError}</Banner.Description>
            </Banner>
          )}
          <NumberField
            value={scoreOf(problem)}
            minValue={0}
            maxValue={problem.maxScore}
            step={1}
            label={`本题得分（满分 ${problem.maxScore}）`}
            disabled={disabled || problem.maxScore <= 0}
            onChange={(value) => setScore(problem, value)}
            onCommit={(value) => setScore(problem, value)}
          />
          <Disclosure title="题面">
            <Html
              html={problem.description}
              base={`${site.url}#/contest/${contestId}`}
              siteId={site.id}
            />
          </Disclosure>
          <ProblemAnswer problem={problem} desktop={desktop} site={site} contestId={contestId} />
          {(problem.kind === 0 || problem.kind === 3 || problem.kind === 4) && (
            <div className="oj-ta-comment">
              <TextField
                type="multiline"
                label="教师评语"
                value={comments[key(problem)] ?? ""}
                {...bindField((value) => setComments((old) => ({ ...old, [key(problem)]: value })))}
                disabled={disabled}
              />
              <Button
                variant="tonal"
                disabled={disabled || isPending(`comment:${key(problem)}`)}
                onClick={() => void saveComment(problem)}
              >
                提交评语
              </Button>
            </div>
          )}
        </div>
      </Card>
      <Disclosure title="评分总览">
        <div className="oj-ta-score-overview">
          {detail.problems.map((row) => (
            <div key={key(row)} className="oj-ta-score-row">
              <span>
                {row.displayLabel}. {row.title}
              </span>
              <NumberField
                value={scoreOf(row)}
                minValue={0}
                maxValue={row.maxScore}
                step={1}
                disabled={row.submissionId === "-1" || row.maxScore <= 0}
                onChange={(value) => setScore(row, value)}
                onCommit={(value) => setScore(row, value)}
              />
              <span className="oj-muted">/ {row.maxScore}</span>
            </div>
          ))}
        </div>
      </Disclosure>
      <div className="oj-actions oj-ta-save">
        <Button
          disabled={uncertain || isPending("save") || state.status === "loading"}
          onClick={() => setConfirmOpen(true)}
        >
          保存并标记已阅
        </Button>
      </div>
      {confirmOpen &&
        typeof document !== "undefined" &&
        createPortal(
          <Dialog open={confirmOpen} headline="确认保存批阅" onClose={() => setConfirmOpen(false)}>
            <div className="space-y-2">
              {detail.checked && (
                <Banner status="warning">
                  <Banner.Description>该学生已标记已阅，保存会覆盖现有分数。</Banner.Description>
                </Banner>
              )}
              {changed.length === 0 ? (
                <p>本次没有修改分数，将直接标记为已阅。</p>
              ) : (
                changed.map((row) => (
                  <p key={key(row)}>
                    {row.displayLabel}. {row.title}：{row.score} → {scoreOf(row)}
                  </p>
                ))
              )}
            </div>
            <div slot="actions">
              <Button variant="text" onClick={() => setConfirmOpen(false)}>
                取消
              </Button>
              <Button disabled={isPending("save")} onClick={() => void save()}>
                {isPending("save") ? "保存中…" : "确认保存"}
              </Button>
            </div>
          </Dialog>,
          document.body,
        )}
    </section>
  );
}

function ProblemAnswer({
  problem,
  desktop,
  site,
  contestId,
}: {
  problem: ReviewProblem;
  desktop: boolean;
  site: Site;
  contestId: string;
}) {
  if (problem.kind === 0 || problem.kind === 4) {
    return (
      <div className="oj-ta-answer">
        <div className="oj-actions">
          <strong>学生代码</strong>
          <Verdict status={problem.status ?? undefined} empty="未提交" />
        </div>
        {problem.code ? (
          <TaCode
            value={problem.code}
            language={problem.language}
            desktop={desktop}
            ariaLabel={`${problem.displayLabel} 学生代码`}
          />
        ) : (
          <p className="oj-muted">本题没有代码提交。</p>
        )}
        {problem.submissionId && problem.submissionId !== "-1" && (
          <Disclosure title="判题详情 / 隐藏输出">
            <TaSubmissionDetail
              key={problem.submissionId}
              site={site}
              contestId={contestId}
              problem={problem}
              submissionId={problem.submissionId}
              desktop={desktop}
            />
          </Disclosure>
        )}
        {problem.referenceAnswer != null && (
          <Disclosure title="参考答案">
            {typeof problem.referenceAnswer === "string" ? (
              <TaCode
                value={problem.referenceAnswer}
                language={problem.language}
                desktop={desktop}
                ariaLabel={`${problem.displayLabel} 参考答案`}
              />
            ) : (
              <pre className="oj-ta-text">{JSON.stringify(problem.referenceAnswer, null, 2)}</pre>
            )}
          </Disclosure>
        )}
      </div>
    );
  }
  if (problem.kind === 3) {
    return (
      <div className="oj-ta-answer">
        <strong>学生答案</strong>
        <pre className="oj-ta-text">
          {typeof problem.answer === "string" && problem.answer ? problem.answer : "未提交本题答案"}
        </pre>
        {problem.referenceAnswer != null && (
          <Disclosure title="参考答案">
            <pre className="oj-ta-text">
              {typeof problem.referenceAnswer === "string"
                ? problem.referenceAnswer
                : JSON.stringify(problem.referenceAnswer, null, 2)}
            </pre>
          </Disclosure>
        )}
      </div>
    );
  }
  if (problem.kind === 1) {
    const selected = Array.isArray(problem.answer) ? (problem.answer as number[]) : [];
    const reference = Array.isArray(problem.referenceAnswer)
      ? (problem.referenceAnswer as number[])
      : [];
    return (
      <div className="oj-ta-answer">
        <strong>学生答案</strong>
        {problem.choices.map((choice, index) => (
          <div key={index} className="oj-ta-option">
            <Chip color={selected.includes(index) ? "accent" : "default"}>
              {String.fromCharCode(65 + index)}
            </Chip>
            <span>{choice}</span>
            {reference.includes(index) && <Chip color="success">参考答案</Chip>}
          </div>
        ))}
        {selected.length === 0 && <p className="oj-muted">未作答。</p>}
      </div>
    );
  }
  if (problem.kind === 2) {
    const answers = Array.isArray(problem.answer) ? (problem.answer as string[]) : [];
    const reference = Array.isArray(problem.referenceAnswer)
      ? (problem.referenceAnswer as string[])
      : [];
    return (
      <div className="oj-ta-answer">
        <strong>学生答案</strong>
        {Array.from({ length: Math.max(problem.blanks, answers.length) }).map((_, index) => (
          <div key={index} className="oj-ta-blank">
            <span>空 {index + 1}</span>
            <pre className="oj-ta-text">{answers[index] ?? "（未填）"}</pre>
            {reference[index] != null && <p className="oj-muted">参考答案：{reference[index]}</p>}
          </div>
        ))}
      </div>
    );
  }
  return null;
}
