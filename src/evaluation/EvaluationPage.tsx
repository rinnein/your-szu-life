import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { Button, Card, Dialog, Divider, Progress, TextField } from "material/react";
import {
  LuCalendarClock,
  LuClipboardCheck,
  LuListChecks,
  LuRefreshCw,
  LuTriangleAlert,
  LuUsers,
} from "react-icons/lu";
import { errorMessage, useLatestRequest } from "../async-state";
import { ModuleHeader, PageShell, SkeletonTable, StatCard } from "../components";
import { createTaskChannel } from "../tauri-channel";
import { usePending } from "../use-pending";
import { Banner, bindField, Chip, Disclosure, SettingList, SettingRow, toast } from "../ui";
import type {
  EvaluationConfig,
  EvaluationOverview,
  EvaluationRunEvent,
  RunFailure,
  RunOutcome,
  TargetStatus,
} from "./model";

const STATUS: Record<TargetStatus, { label: string; color: string }> = {
  pending: { label: "待评", color: "warning" },
  done: { label: "已完成", color: "success" },
  expired: { label: "已过期", color: "default" },
  unsupported: { label: "不支持自动作答", color: "danger" },
};

function deadlineOf(overview: EvaluationOverview | undefined): string {
  if (!overview) return "—";
  const now = new Date();
  const today = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(
    now.getDate(),
  ).padStart(2, "0")}`;
  const dates = overview.targets
    .flatMap((target) => target.records)
    .filter((record) => record.participate)
    .map((record) => record.expiredDate)
    .filter((date): date is string => !!date && date >= today)
    .sort();
  return dates[0] ?? "—";
}

export default function EvaluationPage() {
  // 首次进入与提交后都读缓存；只有顶栏「刷新」显式绕过 30 分钟 TTL。
  const forceRef = useRef(false);
  const { state, run } = useLatestRequest<EvaluationOverview>(() =>
    invoke<EvaluationOverview>("evaluation_overview", { force: forceRef.current }),
  );
  const reload = useCallback(
    (force = false) => {
      forceRef.current = force;
      return run();
    },
    [run],
  );
  useEffect(() => {
    void reload(false);
  }, [reload]);

  const pending = usePending();
  const [comment, setComment] = useState("");
  const [commentLoaded, setCommentLoaded] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [progress, setProgress] = useState<{
    index: number;
    total: number;
    label: string;
  } | null>(null);
  const [waiting, setWaiting] = useState<number | null>(null);
  const [failures, setFailures] = useState<RunFailure[]>([]);

  const overview =
    state.status === "ready"
      ? state.data
      : state.status === "loading" || state.status === "error"
        ? state.previous
        : undefined;
  const loading = state.status === "loading";
  const running = pending.isPending("run");

  useEffect(() => {
    if (!overview || commentLoaded) return;
    setComment(overview.comment);
    setCommentLoaded(true);
  }, [overview, commentLoaded]);

  const nearestDeadline = useMemo(() => deadlineOf(overview), [overview]);
  const pendingCount = overview?.pendingRecords ?? 0;
  const submittedComment = comment.trim() || "无";
  const percent = progress
    ? progress.total > 0
      ? Math.round(((progress.index - 1) / progress.total) * 100)
      : 0
    : 0;

  const saveComment = async () => {
    const outcome = await pending.run("save-config", () =>
      invoke<EvaluationConfig>("evaluation_save_config", { comment }),
    );
    if (!outcome.ok) {
      if (!outcome.skipped) {
        toast.danger("保存评教配置失败", { description: errorMessage(outcome.error) });
      }
      return;
    }
    setComment(outcome.value.comment);
    toast.success("已保存开放题回答", {
      description: outcome.value.comment.trim() || "空值提交时会回退为「无」",
    });
  };

  const submit = async () => {
    setConfirmOpen(false);
    setFailures([]);
    setWaiting(null);
    setProgress({ index: 0, total: overview?.pendingTargets ?? 0, label: "" });
    const channel = createTaskChannel<EvaluationRunEvent>((event) => {
      if (event.type === "target") {
        const update = event.data;
        setProgress({ index: update.index, total: update.total, label: update.targetName });
        if (update.state === "ok") setWaiting(null);
      } else if (event.type === "waiting") {
        setWaiting(event.data.seconds);
      }
    });
    const outcome = await pending.run("run", () =>
      invoke<RunOutcome>("evaluation_run", {
        channel,
        // 空输入也显式传空串：后端据此回退「无」，而不是读取旧配置。
        comment: comment.trim(),
      }),
    );
    setWaiting(null);
    setProgress(null);
    if (!outcome.ok) {
      if (!outcome.skipped) {
        toast.danger("评教提交失败", { description: errorMessage(outcome.error) });
      }
      return;
    }
    const summary = outcome.value.summary;
    if (summary.failures.length > 0) {
      setFailures(summary.failures);
      toast.warning("评教已部分完成", {
        description: `已提交 ${summary.submittedTargets} 个目标，${summary.failures.length} 个目标失败`,
      });
    } else {
      toast.success("评教已完成", {
        description: `已提交 ${summary.submittedTargets} 个目标、${summary.submittedRecords} 位教师`,
      });
    }
    // 后端提交后已经回读过一次概览，这里读缓存即可，避免重复请求。
    void reload(false);
    // 首页待办与评教共用缓存，提交后立即触发一次待办广播。
    void invoke("refresh_todos").catch(() => undefined);
  };

  return (
    <PageShell>
      <ModuleHeader
        title="教学评价"
        description="学生评教清单与一键完成"
        actions={
          <Button
            size="xs"
            variant="tonal"
            disabled={loading || running}
            onClick={() => void reload(true)}
          >
            <LuRefreshCw />
            刷新
          </Button>
        }
      />

      <div className="mx-auto grid w-full max-w-[1100px] min-w-0 gap-5">
        {state.status === "error" && (
          <Banner status="danger">
            <Banner.Title>评教数据加载失败</Banner.Title>
            <Banner.Description>
              {state.message}
              {overview ? " 当前展示的是上一次成功加载的数据。" : ""}
            </Banner.Description>
          </Banner>
        )}

        <div className="grid auto-rows-fr items-stretch gap-3 sm:grid-cols-2 lg:grid-cols-4">
          <StatCard
            label="待评目标"
            value={overview?.pendingTargets ?? 0}
            icon={<LuClipboardCheck />}
            description="当前学期的评价批次"
          />
          <StatCard
            label="待评教师"
            value={pendingCount}
            icon={<LuUsers />}
            color={pendingCount > 0 ? "warning" : "success"}
            description="每条课程-教师记录算一项"
          />
          <StatCard
            label="已过期记录"
            value={overview?.expiredRecords ?? 0}
            icon={<LuTriangleAlert />}
            color={(overview?.expiredRecords ?? 0) > 0 ? "danger" : "default"}
            description="超过截止日期，无法再提交"
          />
          <StatCard
            label="最近截止"
            value={nearestDeadline}
            description="来自评教记录截止日期"
            icon={<LuCalendarClock />}
          />
        </div>

        <Card variant="outlined">
          <div slot="headline">一键评教</div>
          <div slot="subhead">按统一档位为全部待评教师作答，提交后不可撤销。</div>
          <div className="grid gap-3">
            <p className="text-sm text-muted">
              每题按选项分值区间的「次高档」作答（4 档时为良好 / 比较清楚 / 比较符合 /
              比较满意），并为每条参评记录写入开放题回答「{submittedComment}」。
            </p>
            {waiting != null ? (
              <p className="text-xs text-warning" role="status">
                平台要求页面停留，还需等待 {waiting} 秒。
              </p>
            ) : null}
            {running ? (
              <div className="grid gap-2">
                <Progress variant="linear" value={percent} max={100} aria-label="评教进度" />
                <p className="text-xs text-muted" role="status">
                  {progress && progress.label
                    ? `正在提交 ${progress.index}/${progress.total}：${progress.label}`
                    : "正在准备评教数据…"}
                </p>
              </div>
            ) : null}
            <div className="flex flex-wrap items-center gap-2">
              <Button
                variant="filled"
                disabled={running || loading || pendingCount === 0}
                onClick={() => setConfirmOpen(true)}
              >
                开始评教
              </Button>
              {pendingCount === 0 && overview ? (
                <span className="text-xs text-muted">当前没有待评记录</span>
              ) : null}
            </div>
          </div>
        </Card>

        {failures.length > 0 ? (
          <Card variant="outlined">
            <div slot="headline">提交失败的目标</div>
            <ul className="grid gap-1.5">
              {failures.map((failure, index) => (
                <li key={`${failure.targetName}-${index}`} className="text-sm">
                  <span className="font-medium">{failure.targetName}</span>
                  {failure.courseName ? ` · ${failure.courseName}` : ""}
                  <span className="text-muted"> — {failure.message}</span>
                </li>
              ))}
            </ul>
          </Card>
        ) : null}

        <Card variant="outlined">
          <div slot="headline">评教设置</div>
          <div slot="subhead">开放题回答会写入每条参评记录；留空时提交仍会回退为「无」。</div>
          <SettingList>
            <SettingRow
              icon={<LuListChecks aria-hidden="true" />}
              title="开放题回答"
              description="「为了帮助教师改进教学，请写下你对教师或课程的意见和建议」的固定回答。"
            >
              <div className="flex items-center gap-2">
                <TextField
                  label="开放题回答"
                  aria-label="开放题回答"
                  value={comment}
                  disabled={pending.isPending("save-config")}
                  {...bindField(setComment)}
                />
                <Button
                  variant="tonal"
                  disabled={pending.isPending("save-config")}
                  onClick={() => void saveComment()}
                >
                  保存
                </Button>
              </div>
            </SettingRow>
          </SettingList>
        </Card>

        <Divider />

        <section className="grid gap-3" aria-label="评价清单">
          <div className="flex items-center justify-between gap-2">
            <h2 className="text-base font-semibold">评价清单</h2>
            <span className="text-xs text-muted">
              {overview ? `${overview.targets.length} 个目标` : "正在读取…"}
            </span>
          </div>

          {!overview && loading ? (
            <Card variant="outlined">
              <div className="p-6">
                <SkeletonTable rows={4} cols={3} />
              </div>
            </Card>
          ) : null}

          {overview && overview.targets.length === 0 && !loading ? (
            <Card variant="outlined">
              <div className="grid min-h-[180px] place-content-center justify-items-center gap-2 p-6 text-center">
                <LuClipboardCheck className="text-2xl text-muted" aria-hidden="true" />
                <h2 className="text-lg font-semibold">本学期没有评教任务</h2>
                <p className="max-w-md text-sm text-muted">
                  当前学期没有待评或已评记录。评教开放后重新打开本页即可看到清单。
                </p>
              </div>
            </Card>
          ) : null}

          {overview?.targets.map((target) => {
            const status = STATUS[target.status];
            return (
              <Card key={target.id} variant="outlined">
                <Disclosure
                  defaultOpen={target.status === "pending"}
                  ariaLabel={`展开评价目标 ${target.targetName || target.id}`}
                  title={
                    <span className="flex flex-wrap items-center gap-2">
                      <span className="min-w-0 truncate">{target.targetName || "评价目标"}</span>
                      <Chip size="sm" color={status.color} variant="soft">
                        {status.label}
                      </Chip>
                      <span className="text-xs text-muted">{target.termName}</span>
                    </span>
                  }
                >
                  <div className="grid gap-3 text-sm">
                    {target.message ? <p className="text-warning">{target.message}</p> : null}
                    <p className="text-xs text-muted">
                      {target.questions.length} 道题 · {target.records.length} 条记录 · 开放题
                      {target.commentRequired === 2 ? "必填" : "可选"}
                    </p>
                    {target.records.length > 0 ? (
                      <ul className="grid gap-1.5">
                        {target.records.map((record) => (
                          <li
                            key={record.scId}
                            className="flex flex-wrap items-center justify-between gap-2 rounded-inner bg-surface-secondary px-3 py-2"
                          >
                            <span className="min-w-0 truncate">
                              {record.employeeName} · {record.courseName}
                              <span className="ml-2 text-xs text-muted">{record.courseNo}</span>
                            </span>
                            <span className="text-xs text-muted tabular-nums">
                              {record.participate
                                ? `已评 ${record.currentTimes}/${record.limitedTimes}`
                                : "不参评"}
                              {" · "}截止 {record.expiredDate ?? "—"}
                            </span>
                          </li>
                        ))}
                      </ul>
                    ) : null}
                    {target.questions.length > 0 ? (
                      <details className="text-xs text-muted">
                        <summary className="cursor-pointer">查看题目与档位</summary>
                        <ul className="mt-2 grid gap-1">
                          {target.questions.map((question, index) => (
                            <li key={question.subId}>
                              {index + 1}. {question.subName}
                            </li>
                          ))}
                        </ul>
                      </details>
                    ) : null}
                  </div>
                </Disclosure>
              </Card>
            );
          })}
        </section>
      </div>

      {confirmOpen
        ? createPortal(
            <Dialog
              open
              onClose={() => setConfirmOpen(false)}
              headline="确认一键评教"
              subtitle="提交后不可撤销，请先核对范围"
              size="small"
              actions={
                <>
                  <Button variant="text" onClick={() => setConfirmOpen(false)}>
                    取消
                  </Button>
                  <Button variant="filled" onClick={() => void submit()}>
                    确认提交
                  </Button>
                </>
              }
            >
              <div className="grid gap-2 text-sm">
                <p>
                  将提交 <strong>{overview?.pendingTargets ?? 0}</strong> 个评价目标、
                  <strong>{pendingCount}</strong> 位教师。
                </p>
                <p>
                  评价档位：每题选项分值区间的次高档（4 档时为良好 / 比较清楚 / 比较符合 /
                  比较满意）。
                </p>
                <p>开放题回答：{submittedComment}</p>
              </div>
            </Dialog>,
            document.body,
          )
        : null}
    </PageShell>
  );
}
