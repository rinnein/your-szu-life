import { useEffect, useMemo, useState } from "react";
import { useSearch } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Accordion,
  Alert,
  AlertDialog,
  Button,
  Card,
  Chip,
  Checkbox,
  CloseButton,
  Label,
  Modal,
  NumberField,
  ProgressBar,
  Spinner,
  Switch,
  TextArea,
  Tabs,
  toast,
} from "@heroui/react";
import {
  LuArrowLeft,
  LuCheck,
  LuCircleHelp,
  LuMessageCircle,
  LuPause,
  LuPlay,
  LuRefreshCw,
  LuSquare,
  LuTrash2,
} from "react-icons/lu";
import { PageShell, SkeletonCard } from "../components";
import { useLogicalNavigation } from "../logical-navigation";
import { ModelDownloadModal } from "../ModelDownloadModal";
import { useEmbeddingModelStatus } from "../EmbeddingModelSettings";
import { AiSettings, QuizModePicker } from "./AiSettings";
import { QuizPanel } from "./QuizPanel";
import { UoocQaSettings } from "./UoocQaSettings";
import type { AiConfig, QuizMode } from "./types";
import { CourseProgressView } from "./CourseProgressView";
import {
  isUoocSnapshot,
  newerSnapshot,
  taskStatus,
  checkinActive,
  isCourseTaskCompleted,
  buildCourseTaskItems,
  type CourseTaskItem,
  type QuizState,
  type UoocSettings,
  type UoocSnapshot,
  type UoocTask,
} from "./types";

const sessions = {
  checking: "正在检查登录状态",
  ready: "已登录",
  loginRequired: "需要登录",
  networkError: "网络连接失败",
  loggingIn: "等待官方登录完成",
};
const active = (task: UoocTask) => task.status === "running" || task.status === "queued";
const seconds = (value: number) =>
  `${Math.floor(value / 60)}:${Math.floor(value % 60)
    .toString()
    .padStart(2, "0")}`;

export default function UoocPage() {
  const navigation = useLogicalNavigation();
  const { tab, course } = useSearch({ from: "/uooc" });
  const [snapshot, setSnapshot] = useState<UoocSnapshot>();
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [clock, setClock] = useState(() => Date.now());
  const [defaultMode, setDefaultMode] = useState<QuizMode>("supervised");
  const [expandedAccordionKeys, setExpandedAccordionKeys] = useState<Record<string, Set<string>>>(
    {},
  );
  const [selectedQuiz, setSelectedQuiz] = useState<QuizState | null>(null);
  const [modalQuizCid, setModalQuizCid] = useState<string | null>(null);
  const [taskToDelete, setTaskToDelete] = useState<UoocTask | null>(null);
  const [confirmDeleteCompleted, setConfirmDeleteCompleted] = useState(false);
  const embeddingStatus = useEmbeddingModelStatus();
  const [embeddingModalOpen, setEmbeddingModalOpen] = useState(false);
  const [embeddingDismissed, setEmbeddingDismissed] = useState(false);

  useEffect(() => {
    if (
      embeddingStatus &&
      embeddingStatus.status !== "ready" &&
      embeddingStatus.status !== "checking" &&
      !embeddingDismissed
    ) {
      setEmbeddingModalOpen(true);
    }
  }, [embeddingStatus?.status, embeddingDismissed]);

  useEffect(() => {
    if (embeddingStatus?.status === "ready") {
      setEmbeddingModalOpen(false);
    }
  }, [embeddingStatus?.status]);

  useEffect(() => {
    if (course) {
      document.getElementById(`uooc-course-${course}`)?.scrollIntoView?.({ block: "start" });
      setExpandedAccordionKeys((prev) => ({
        ...prev,
        [course]: new Set(["completed-tasks"]),
      }));
      setModalQuizCid(course);
    }
  }, [tab, course]);
  useEffect(() => {
    void invoke<AiConfig>("get_uooc_ai_config")
      .then((v) => {
        if (v?.defaultMode) setDefaultMode(v.defaultMode);
      })
      .catch(() => {});
  }, []);
  const [debugEnabled, setDebugEnabled] = useState(false);
  useEffect(() => {
    let active = true;
    void invoke("get_debug_mode")
      .then((value) => {
        if (active) setDebugEnabled(Boolean((value as { enabled?: boolean })?.enabled));
      })
      .catch(() => undefined);
    let unlisten: (() => void) | undefined;
    void listen<{ enabled: boolean }>("debug-mode-changed", (event) => {
      if (active) setDebugEnabled(event.payload.enabled);
    }).then((stop) => {
      if (active) unlisten = stop;
      else stop();
    });
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);
  const [selected, setSelected] = useState<string[]>([]);
  const [settings, setSettings] = useState<UoocSettings>({
    concurrency: 3,
    reportIntervalSec: 20,
    speedMultiplier: 2,
    dryRun: false,
    discussionEnabled: true,
    discussionContent: "要好好学习",
    discussionIntervalMin: 30,
  });
  const apply = (next: UoocSnapshot) => setSnapshot((old) => newerSnapshot(old, next));

  useEffect(() => {
    let mounted = true;
    let stop: (() => void) | undefined;
    void listen<UoocSnapshot>("uooc-state-changed", ({ payload }) => {
      if (mounted && isUoocSnapshot(payload)) apply(payload);
    })
      .then(async (dispose) => {
        if (!mounted) {
          dispose();
          return;
        }
        stop = dispose;
        const next = await invoke<UoocSnapshot>("get_uooc_snapshot");
        if (mounted) {
          if (isUoocSnapshot(next)) {
            apply(next);
            if (next.session === "ready") {
              void invoke("build_uooc_local_bank").catch(() => {});
              const progress = await invoke("refresh_uooc_progress", { courseIds: [] });
              if (mounted && isUoocSnapshot(progress)) apply(progress);
            }
          } else setError("请在泥深原生应用中使用 UOOC 功能。");
        }
      })
      .catch(() => {
        if (mounted) setError("无法读取 UOOC 服务，请在原生应用中重试。");
      });
    return () => {
      mounted = false;
      stop?.();
    };
  }, []);

  useEffect(() => {
    if (snapshot?.session === "ready") {
      void invoke("build_uooc_local_bank").catch(() => {});
    }
  }, [snapshot?.session]);

  const saved = snapshot?.settings;
  useEffect(() => {
    if (saved) {
      setSettings({
        concurrency: saved.concurrency,
        reportIntervalSec: saved.reportIntervalSec,
        speedMultiplier: saved.speedMultiplier,
        dryRun: saved.dryRun,
        discussionEnabled: saved.discussionEnabled ?? true,
        discussionContent: saved.discussionContent ?? "要好好学习",
        discussionIntervalMin: saved.discussionIntervalMin ?? 30,
        ...(saved.qaServerUrl !== undefined ? { qaServerUrl: saved.qaServerUrl } : {}),
      });
    }
    // Only actual settings changes replace edits, not progress events.
  }, [
    saved?.concurrency,
    saved?.reportIntervalSec,
    saved?.speedMultiplier,
    saved?.dryRun,
    saved?.discussionEnabled,
    saved?.discussionContent,
    saved?.discussionIntervalMin,
    saved?.qaServerUrl,
  ]);

  const action = async (command: string, args?: Record<string, unknown>) => {
    setBusy(true);
    setError(undefined);
    try {
      const next = await invoke<unknown>(command, args);
      if (isUoocSnapshot(next)) apply(next);
      return true;
    } catch (cause) {
      const msg = typeof cause === "string" ? cause : "操作未完成，请重试。";
      setError(msg);
      toast.danger("操作失败", { description: msg });
      return false;
    } finally {
      setBusy(false);
    }
  };
  const ready = snapshot?.session === "ready";
  const refreshDetails = (cid: string) => {
    if (!ready) return;
    void invoke("refresh_uooc_progress", { courseIds: [cid] })
      .then((next) => {
        if (isUoocSnapshot(next)) apply(next);
      })
      .catch((e: unknown) => {
        const msg = typeof e === "string" ? e : "课程详情刷新失败，请重试。";
        setError(msg);
        toast.danger("课程详情刷新失败", { description: msg });
      });
  };
  const tasks =
    snapshot?.tasks.filter((task) => !snapshot.user || task.accountId === snapshot.user.id) ?? [];
  const activeModalQuiz = selectedQuiz
    ? (Object.values(snapshot?.quizzes ?? {}).find(
        (q) =>
          q.cid === selectedQuiz.cid &&
          (!snapshot?.user || q.accountId === snapshot.user.id) &&
          ((q.taskId && q.taskId === selectedQuiz.taskId) ||
            (q.resourceId && q.resourceId === selectedQuiz.resourceId)),
      ) ?? selectedQuiz)
    : modalQuizCid
      ? (Object.values(snapshot?.quizzes ?? {}).find(
          (q) => q.cid === modalQuizCid && (!snapshot?.user || q.accountId === snapshot.user.id),
        ) ?? null)
      : null;
  const running = tasks.some(active);
  const discussionEnabled = settings.discussionEnabled ?? true;
  const discussionContent = settings.discussionContent ?? "要好好学习";
  const discussionIntervalMin = settings.discussionIntervalMin ?? 30;
  const cooldown = snapshot?.discussionCooldownUntilMs;
  useEffect(() => {
    const hasDiscussionTimestamps = Object.values(snapshot?.discussions ?? {}).some(
      (state) => state.lastSentAtMs != null,
    );
    if (!cooldown && !hasDiscussionTimestamps) return;
    const timer = window.setInterval(() => setClock(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [cooldown, snapshot?.discussions]);
  const cooldownRemainingMs = cooldown ? Math.max(0, cooldown - clock) : 0;
  const cooldownText =
    cooldownRemainingMs > 0
      ? `讨论冷却中，${Math.ceil(cooldownRemainingMs / 60000)} 分钟后继续`
      : undefined;
  const cooldownClock =
    cooldownRemainingMs > 0
      ? `${Math.floor(cooldownRemainingMs / 60000)}:${Math.floor(
          (cooldownRemainingMs % 60000) / 1000,
        )
          .toString()
          .padStart(2, "0")}`
      : "现在可以发起";
  const discussionEligible = (cid: string) => {
    const progress = snapshot?.progress?.[cid]?.data?.discussion;
    const ratio =
      progress?.score != null && progress.weight != null && progress.weight > 0
        ? progress.score / progress.weight
        : undefined;
    return ready && ratio != null && ratio < 1;
  };
  const discussionCandidates = useMemo(
    () =>
      snapshot?.courses.filter((course) => {
        const progress = snapshot.progress?.[course.id]?.data?.discussion;
        const ratio =
          progress?.score != null && progress.weight != null && progress.weight > 0
            ? progress.score / progress.weight
            : undefined;
        return ratio != null && ratio < 1;
      }) ?? [],
    [snapshot],
  );
  const discussionStatus = (cid: string) => {
    const state = snapshot?.discussions?.[cid];
    const progress = snapshot?.progress?.[cid]?.data?.discussion;
    const ratio =
      progress?.score != null && progress.weight ? progress.score / progress.weight : undefined;
    if (!state && ratio == null) return undefined;
    if (ratio == null || ratio >= 1) return "无需讨论";
    if (state?.lastSentAtMs) {
      const elapsed = Math.max(0, clock - state.lastSentAtMs);
      return `上次发送讨论于${elapsed < 60_000 ? "不到 1 分钟前" : `${Math.floor(elapsed / 60_000)} 分钟前`}`;
    }
    return "无需讨论";
  };
  const added = new Set(
    snapshot?.tasks
      .filter((task) => task.accountId === snapshot.user?.id)
      .map((task) => task.course.id),
  );
  const selectable =
    snapshot?.courses.filter((course) => !added.has(course.id)).map((course) => course.id) ?? [];
  const ids = selected.filter((id) => selectable.includes(id));
  const pendingCheckins =
    snapshot?.courses
      .filter(
        (course) =>
          snapshot.progress?.[course.id]?.data?.attendance === "needed" &&
          !checkinActive(snapshot.checkins?.[course.id]),
      )
      .map((course) => course.id) ?? [];
  const control = (actionName: "pause" | "resume" | "stop" | "delete", courseIds: string[] = []) =>
    void action("control_uooc_tasks", { courseIds, action: actionName });
  const start = async () => {
    if (window.SzuApplicationBack) {
      const { isPermissionGranted, requestPermission } =
        await import("@tauri-apps/plugin-notification");
      if (!(await isPermissionGranted())) await requestPermission();
    }
    if (await action("start_uooc_courses", { courseIds: ids }))
      await navigation.replace("/uooc?tab=tasks");
  };
  const startDiscussion = async (cid: string) => {
    const ok = await action("start_uooc_discussion", { courseId: cid });
    if (!ok) {
      toast.danger("本次讨论未发送成功，请重试。");
    }
  };
  const checkinControls = (cid: string) => {
    const state = snapshot?.checkins?.[cid];
    const attendance = snapshot?.progress?.[cid]?.data?.attendance ?? "unknown";
    return (
      <div className="grid gap-2">
        <div className="flex flex-wrap gap-2">
          {checkinActive(state) ? (
            <Button
              size="sm"
              variant="tertiary"
              isDisabled={busy}
              onPress={() => void action("cancel_uooc_checkin", { courseIds: [cid] })}
            >
              取消签到
            </Button>
          ) : attendance === "needed" || state?.status === "failed" ? (
            <Button
              size="sm"
              variant="secondary"
              isDisabled={busy || !ready}
              onPress={() => void action("start_uooc_checkin", { courseIds: [cid] })}
            >
              {state?.status === "failed" ? "手动重试签到" : "签到"}
            </Button>
          ) : attendance === "unknown" ? (
            <Button
              size="sm"
              variant="tertiary"
              isDisabled={busy || !ready}
              onPress={() => void action("refresh_uooc_progress", { courseIds: [cid] })}
            >
              查询签到状态
            </Button>
          ) : null}
        </div>
        {state?.message && (
          <p role="status" className="text-xs text-muted">
            {state.source === "manual" ? "手动签到" : "自动签到"}：{state.message}
          </p>
        )}
      </div>
    );
  };
  const discussionControls = (cid: string) => {
    const progress = snapshot?.progress?.[cid]?.data?.discussion;
    const ratio =
      progress?.score != null && progress.weight != null && progress.weight > 0
        ? progress.score / progress.weight
        : undefined;
    const needsDiscussion = ratio != null && ratio < 1;
    return (
      <div className="grid gap-2">
        <div className="flex flex-wrap items-center gap-2 text-xs text-muted">
          <span role="status">
            {discussionStatus(cid) ?? (needsDiscussion ? "无需讨论" : "无需讨论")}
          </span>
        </div>
        {needsDiscussion && (
          <Button
            size="sm"
            variant="secondary"
            isDisabled={busy || !ready || !discussionEligible(cid)}
            onPress={() => startDiscussion(cid)}
          >
            <LuMessageCircle aria-hidden="true" />
            立即参与一次讨论
          </Button>
        )}
      </div>
    );
  };

  return (
    <PageShell>
      <ModelDownloadModal
        kind="embedding"
        isOpen={embeddingModalOpen}
        onOpenChange={(open) => {
          setEmbeddingModalOpen(open);
          if (!open) setEmbeddingDismissed(true);
        }}
      />
      <div className="mx-auto grid w-full max-w-4xl min-w-0 gap-5">
        <header className="flex items-center gap-3">
          <CloseButton
            data-webview-navigation
            aria-label="返回工作台"
            onPress={() => void navigation.back()}
          >
            <LuArrowLeft aria-hidden="true" />
          </CloseButton>
          <h1 data-webview-navigation className="text-2xl font-semibold">
            UOOC刷课
          </h1>
        </header>
        <Card>
          <Card.Header>
            <Card.Title>{snapshot?.user?.name ?? "优课在线"}</Card.Title>
            <Card.Description>
              {snapshot ? sessions[snapshot.session] : "正在连接 UOOC 服务…"}
            </Card.Description>
          </Card.Header>
          <Card.Content className="grid gap-3">
            <div className="flex flex-wrap gap-2">
              <Button
                isDisabled={busy || !snapshot || snapshot.session === "loggingIn"}
                onPress={() => void action("open_uooc_login")}
              >
                {ready ? "恢复官方登录" : "登录 UOOC"}
              </Button>
              <Button
                variant="secondary"
                isDisabled={busy || !snapshot || snapshot.session === "loggingIn"}
                onPress={() => void action("refresh_uooc_session")}
              >
                <LuRefreshCw aria-hidden="true" />
                刷新课程
              </Button>
              {snapshot?.user && (
                <Button
                  variant="tertiary"
                  isDisabled={busy}
                  onPress={() => void action("logout_uooc")}
                >
                  退出 UOOC
                </Button>
              )}
              {busy && <Spinner aria-label="正在处理" size="sm" />}
            </div>
            {snapshot?.message && (
              <p role="status" className="text-sm text-muted">
                {snapshot.message}
              </p>
            )}
          </Card.Content>
        </Card>
        <Tabs
          selectedKey={tab}
          onSelectionChange={(key) => void navigation.replace(`/uooc?tab=${key}`)}
        >
          <Tabs.ListContainer>
            <Tabs.List aria-label="UOOC 功能">
              <Tabs.Tab id="courses">
                开课中
                <Tabs.Indicator />
              </Tabs.Tab>
              <Tabs.Tab id="tasks">
                任务{tasks.length ? ` (${tasks.length})` : ""}
                <Tabs.Indicator />
              </Tabs.Tab>
              <Tabs.Tab id="qa">
                题库
                <Tabs.Indicator />
              </Tabs.Tab>
              <Tabs.Tab id="settings">
                运行设置
                <Tabs.Indicator />
              </Tabs.Tab>
            </Tabs.List>
          </Tabs.ListContainer>
          <Tabs.Panel id="courses" className="grid gap-6 pt-4">
            {!snapshot && !error && <SkeletonCard />}
            <p className="text-sm text-muted">
              已登录且应用运行期间，按北京时间每日自动为有签到考核的待签到课程提交签到，无需加入视频任务。
              提交后根据服务端签到进度确认结果。
            </p>
            {pendingCheckins.length > 0 && (
              <Alert status="warning" className="flex flex-wrap items-center justify-between gap-3">
                <Alert.Indicator />
                <Alert.Content>
                  <Alert.Title>需要签到</Alert.Title>
                  <Alert.Description>{pendingCheckins.length} 门课程需要签到</Alert.Description>
                </Alert.Content>
                <Button
                  variant="secondary"
                  isDisabled={!ready || busy}
                  onPress={() => void action("start_uooc_checkin", { courseIds: pendingCheckins })}
                >
                  签到全部待签到课程 ({pendingCheckins.length})
                </Button>
              </Alert>
            )}
            <div className="flex flex-wrap items-center justify-between gap-3">
              <p className="text-sm text-muted">{snapshot?.courses.length ?? 0} 门开课中的课程</p>
              <div className="flex flex-wrap gap-2">
                <Button
                  variant="tertiary"
                  isDisabled={!ready || selectable.length === 0}
                  onPress={() => setSelected(ids.length === selectable.length ? [] : selectable)}
                >
                  {ids.length > 0 && ids.length === selectable.length ? "取消全选" : "全选"}
                </Button>
                <Button
                  isDisabled={!ready || busy || ids.length === 0}
                  onPress={() =>
                    void start().catch(() => setError("无法启动任务，请检查通知权限后重试。"))
                  }
                >
                  <LuPlay aria-hidden="true" />
                  {snapshot?.settings.dryRun ? "预览所选" : "开始所选"} ({ids.length})
                </Button>
              </div>
            </div>
            {snapshot?.courses.map((course) => {
              const testWeight = snapshot.progress?.[course.id]?.data?.test?.weight;
              const hasTestWeight = typeof testWeight === "number" && testWeight > 0;
              return (
                <Card key={course.id}>
                  <Card.Content className="grid gap-2">
                    <Checkbox
                      isSelected={added.has(course.id) || ids.includes(course.id)}
                      isDisabled={!ready || busy || added.has(course.id)}
                      onChange={(checked) =>
                        setSelected((old) =>
                          checked
                            ? [...new Set([...old, course.id])]
                            : old.filter((id) => id !== course.id),
                        )
                      }
                    >
                      <Checkbox.Content>
                        <Checkbox.Control>
                          <Checkbox.Indicator />
                        </Checkbox.Control>
                        {course.title}
                      </Checkbox.Content>
                    </Checkbox>
                    <QuizModePicker
                      value={
                        Object.values(snapshot?.quizzes ?? {}).find(
                          (q) => q.cid === course.id && q.accountId === snapshot?.user?.id,
                        )?.mode ?? defaultMode
                      }
                      disabled={
                        !hasTestWeight ||
                        !ready ||
                        busy ||
                        tasks.some((t) => t.course.id === course.id && active(t))
                      }
                      description={!hasTestWeight ? "测验无权重占比，无需配置答题模式" : undefined}
                      onChange={(mode) =>
                        void action("save_uooc_quiz_mode", { cid: course.id, mode })
                      }
                    />
                    {added.has(course.id) && (
                      <Chip size="sm" className="justify-self-start">
                        已添加
                      </Chip>
                    )}
                    <p className="text-sm text-muted">
                      {course.organization} · {course.semester}
                    </p>
                    <CourseProgressView
                      course={course}
                      entry={snapshot?.progress?.[course.id]}
                      onRefresh={() => refreshDetails(course.id)}
                    />
                    {checkinControls(course.id)}
                    {discussionControls(course.id)}
                  </Card.Content>
                </Card>
              );
            })}
            {snapshot && !snapshot.courses.length && (
              <Card>
                <Card.Content>
                  <p className="text-muted">
                    {ready ? "当前没有开课中的课程。" : "登录 UOOC 后查看开课中的课程。"}
                  </p>
                </Card.Content>
              </Card>
            )}
          </Tabs.Panel>
          <Tabs.Panel id="tasks" className="grid gap-4 pt-4">
            <div className="flex flex-wrap gap-2">
              <Button
                variant="secondary"
                isDisabled={busy || !running}
                onPress={() => control("pause")}
              >
                <LuPause aria-hidden="true" />
                暂停全部
              </Button>
              <Button
                variant="secondary"
                isDisabled={busy || !ready || !tasks.length}
                onPress={() => control("resume")}
              >
                <LuPlay aria-hidden="true" />
                继续全部
              </Button>
              <Button
                variant="tertiary"
                isDisabled={busy || !tasks.length}
                onPress={() => control("stop")}
              >
                <LuSquare aria-hidden="true" />
                停止全部
              </Button>
              {tasks.some((t) => t.status === "completed") && (
                <Button
                  variant="tertiary"
                  isDisabled={busy}
                  className="text-danger hover:text-danger hover:bg-danger/10"
                  onPress={() => setConfirmDeleteCompleted(true)}
                >
                  <LuTrash2 aria-hidden="true" />
                  删除已完成任务
                </Button>
              )}
            </div>
            <p className="text-sm text-muted">
              离开本页后任务继续运行。重启时自动恢复上次运行的课程；主动暂停或停止的任务不会自动恢复。
            </p>
            <section aria-labelledby="uooc-course-tasks" className="grid gap-4">
              <div className="flex items-end justify-between gap-3 border-b border-separator/60 pb-2">
                <div>
                  <h2 id="uooc-course-tasks" className="text-lg font-semibold">
                    课程 / 测验
                  </h2>
                  <p className="text-sm text-muted">视频任务、前置测验和课程进度集中在这里。</p>
                </div>
                <span className="text-xs text-muted">{tasks.length} 门课程</span>
              </div>
              {tasks.map((task) => {
                const isCompleted = isCourseTaskCompleted(
                  task,
                  snapshot?.progress?.[task.course.id],
                );
                const taskQuiz = Object.values(snapshot?.quizzes ?? {}).find(
                  (q) => q.cid === task.course.id && q.accountId === task.accountId,
                );
                const taskItems = buildCourseTaskItems(task.videos, taskQuiz);
                const isItemCompleted = (item: CourseTaskItem): boolean =>
                  item.kind === "video"
                    ? item.video.status === "completed"
                    : item.quiz.stage === "passed" || item.quiz.videoComplete;
                const completedItems = taskItems.filter(isItemCompleted);
                const pendingItems = taskItems.filter((item) => !isItemCompleted(item));
                const completedKeys = expandedAccordionKeys[task.course.id] ?? new Set<string>([]);

                const renderTaskItem = (item: CourseTaskItem) =>
                  item.kind === "quiz" ? (
                    <div
                      key={item.id}
                      role="button"
                      tabIndex={0}
                      onClick={() => setSelectedQuiz(item.quiz)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                          e.preventDefault();
                          setSelectedQuiz(item.quiz);
                        }
                      }}
                      className="grid gap-1.5 p-3 rounded-lg border border-separator/60 hover:border-accent/60 hover:bg-surface/60 transition-colors cursor-pointer group"
                      aria-label={`测验：${item.quiz.title || "课程测验"}，点击打开测验`}
                    >
                      <div className="flex flex-wrap items-center justify-between gap-2 text-sm">
                        <div className="flex items-center gap-2">
                          <LuCircleHelp className="text-accent shrink-0" aria-hidden="true" />
                          <span className="font-medium group-hover:text-accent transition-colors">
                            {item.quiz.title || "课程测验"}
                          </span>
                        </div>
                        {item.quiz.stage === "passed" ? (
                          <Chip color="success" size="sm">
                            <LuCheck aria-hidden="true" />
                            <Chip.Label>已完成</Chip.Label>
                          </Chip>
                        ) : ["preparing", "generating", "submitting", "verifying"].includes(
                            item.quiz.stage,
                          ) ? (
                          <div className="flex items-center gap-1.5">
                            <Spinner size="sm" aria-label="处理中" />
                            <span className="text-xs text-muted">处理中</span>
                          </div>
                        ) : (
                          <Chip color="warning" size="sm">
                            <Chip.Label>
                              {item.quiz.stage === "awaitingApproval"
                                ? "待核对"
                                : item.quiz.stage === "awaitingChoice"
                                  ? "待答题"
                                  : "点击答题"}
                            </Chip.Label>
                          </Chip>
                        )}
                      </div>
                      <div className="flex items-center justify-between text-xs text-muted">
                        <span>{item.quiz.message || "点击打开弹窗查看和完成题目"}</span>
                        <span className="text-accent text-[11px] group-hover:underline">
                          打开测验 ›
                        </span>
                      </div>
                    </div>
                  ) : (
                    <div key={item.video.resourceId} className="grid gap-1">
                      <div className="flex flex-wrap justify-between gap-2 text-sm">
                        <span>{item.video.title}</span>
                        {item.video.status === "completed" ? (
                          <Chip color="success" size="sm">
                            <LuCheck aria-hidden="true" />
                            <Chip.Label>已刷完</Chip.Label>
                          </Chip>
                        ) : (
                          <span className="text-muted">{taskStatus[item.video.status]}</span>
                        )}
                      </div>
                      <ProgressBar
                        aria-label={`${item.video.title}进度`}
                        value={
                          item.video.status === "completed"
                            ? 100
                            : item.video.duration
                              ? Math.min(100, (item.video.position / item.video.duration) * 100)
                              : 0
                        }
                        color={item.video.status === "completed" ? "success" : "accent"}
                        isIndeterminate={!item.video.duration && item.video.status === "running"}
                      >
                        <ProgressBar.Track>
                          <ProgressBar.Fill />
                        </ProgressBar.Track>
                      </ProgressBar>
                      <p className="text-xs text-muted">
                        {seconds(item.video.position)} /{" "}
                        {item.video.duration
                          ? seconds(item.video.duration)
                          : item.video.status === "completed"
                            ? "时长未记录"
                            : "等待时长"}
                        {item.video.message && ` · ${item.video.message}`}
                      </p>
                    </div>
                  );

                return (
                  <Card key={task.course.id} id={`uooc-course-${task.course.id}`}>
                    <Card.Header>
                      <Card.Title>{task.course.title}</Card.Title>
                      {isCompleted && (
                        <Chip color="success" size="sm" className="justify-self-start">
                          <LuCheck aria-hidden="true" />
                          <Chip.Label>视频已刷完</Chip.Label>
                        </Chip>
                      )}
                      <Card.Description>
                        {taskStatus[task.status]} · 完成{" "}
                        {task.videos.filter((v) => v.status === "completed").length} /{" "}
                        {task.videos.length} 个视频
                      </Card.Description>
                    </Card.Header>
                    <Card.Content className="grid gap-3">
                      <CourseProgressView
                        course={task.course}
                        entry={snapshot?.progress?.[task.course.id]}
                        onRefresh={() => refreshDetails(task.course.id)}
                      />
                      {checkinControls(task.course.id)}
                      {discussionControls(task.course.id)}
                      <div className="flex flex-wrap gap-2">
                        <Button
                          size="sm"
                          variant="secondary"
                          isDisabled={busy || !active(task)}
                          onPress={() => control("pause", [task.course.id])}
                        >
                          暂停
                        </Button>
                        <Button
                          size="sm"
                          variant="secondary"
                          isDisabled={busy || !ready || active(task)}
                          onPress={() => control("resume", [task.course.id])}
                        >
                          {task.status === "completed" ? "继续测验" : "继续"}
                        </Button>
                        <Button
                          size="sm"
                          variant="tertiary"
                          isDisabled={busy || task.status === "stopped"}
                          onPress={() => control("stop", [task.course.id])}
                        >
                          停止
                        </Button>
                        <Button
                          size="sm"
                          variant="tertiary"
                          isDisabled={busy}
                          className="text-danger hover:text-danger hover:bg-danger/10"
                          onPress={() => setTaskToDelete(task)}
                        >
                          <LuTrash2 aria-hidden="true" />
                          删除
                        </Button>
                      </div>
                      {task.message && (
                        <p role="status" className="text-sm text-muted">
                          {task.message}
                        </p>
                      )}
                      <div className="grid max-h-96 gap-3 overflow-auto">
                        {completedItems.length > 0 && (
                          <Accordion
                            variant="surface"
                            expandedKeys={completedKeys}
                            onExpandedChange={(keys) =>
                              setExpandedAccordionKeys((prev) => ({
                                ...prev,
                                [task.course.id]: new Set(Array.from(keys).map(String)),
                              }))
                            }
                          >
                            <Accordion.Item id="completed-tasks">
                              <Accordion.Heading>
                                <Accordion.Trigger
                                  aria-label={`已刷完任务 (${completedItems.length} 项)`}
                                >
                                  <div className="flex flex-1 items-center gap-2 text-sm font-medium">
                                    <LuCheck className="shrink-0 text-success" aria-hidden="true" />
                                    <span>已刷完任务 ({completedItems.length} 项)</span>
                                    <Chip color="success" size="sm">
                                      <Chip.Label>已刷完</Chip.Label>
                                    </Chip>
                                  </div>
                                  <Accordion.Indicator />
                                </Accordion.Trigger>
                              </Accordion.Heading>
                              <Accordion.Panel>
                                <Accordion.Body className="grid gap-3 pt-2">
                                  {completedItems.map(renderTaskItem)}
                                </Accordion.Body>
                              </Accordion.Panel>
                            </Accordion.Item>
                          </Accordion>
                        )}
                        {pendingItems.map(renderTaskItem)}
                        {taskItems.length === 0 && <p className="text-sm text-muted">暂无任务项</p>}
                      </div>
                    </Card.Content>
                  </Card>
                );
              })}
            </section>
            <section aria-labelledby="uooc-discussion-tasks" className="grid gap-4">
              <div className="flex items-end justify-between gap-3 border-b border-separator/60 pb-2">
                <div>
                  <h2 id="uooc-discussion-tasks" className="text-lg font-semibold">
                    讨论
                  </h2>
                  <p className="text-sm text-muted">
                    课程之间共享发送间隔，下一次只会发起一个课程的讨论。
                  </p>
                </div>
                <LuMessageCircle className="text-muted" aria-hidden="true" />
              </div>
              <Card>
                <Card.Content className="grid gap-3 sm:grid-cols-[1fr_auto] sm:items-center">
                  <div className="grid gap-1">
                    <span className="text-xs text-muted">下一次发起讨论</span>
                    <strong className="text-base">
                      {discussionCandidates[0]?.title ?? "暂无待完成的讨论课程"}
                    </strong>
                    <span className="text-sm text-muted">
                      {!discussionEnabled
                        ? "自动讨论已关闭，可使用课程卡片中的单次参与"
                        : discussionCandidates.length
                          ? (cooldownText ?? "满足条件后会自动发起")
                          : "刷新课程进度后重新检查"}
                    </span>
                  </div>
                  <div className="rounded-lg border border-separator/60 px-4 py-3 text-center">
                    <span
                      className="block font-mono text-xl font-semibold tabular-nums"
                      aria-live="polite"
                    >
                      {discussionCandidates.length ? cooldownClock : "暂空"}
                    </span>
                    <span className="text-xs text-muted">全局冷却</span>
                  </div>
                </Card.Content>
              </Card>
            </section>
            {!tasks.length && (
              <Card>
                <Card.Content>选择课程并开始后，任务会显示在这里。</Card.Content>
              </Card>
            )}
            <Modal
              isOpen={!!activeModalQuiz}
              onOpenChange={(open) => {
                if (!open) {
                  setSelectedQuiz(null);
                  setModalQuizCid(null);
                }
              }}
            >
              <Modal.Backdrop>
                <Modal.Container size="cover" className="max-w-4xl">
                  <Modal.Dialog aria-label={activeModalQuiz?.title || "课程测验"} className="flex flex-col max-h-[85vh]">
                    <Modal.CloseTrigger />
                    {activeModalQuiz && (
                      <QuizPanel
                        quiz={activeModalQuiz}
                        active={tasks.some(
                          (t) => t.course.id === activeModalQuiz.cid && active(t),
                        )}
                        weight={snapshot?.progress?.[activeModalQuiz.cid]?.data?.test.weight}
                        dryRun={snapshot?.settings.dryRun ?? false}
                        onSnapshot={apply}
                        onClose={() => {
                          setSelectedQuiz(null);
                          setModalQuizCid(null);
                        }}
                      />
                    )}
                  </Modal.Dialog>
                </Modal.Container>
              </Modal.Backdrop>
            </Modal>
            <AlertDialog
              isOpen={!!taskToDelete}
              onOpenChange={(open) => {
                if (!open) setTaskToDelete(null);
              }}
            >
              <AlertDialog.Backdrop>
                <AlertDialog.Container size="sm">
                  <AlertDialog.Dialog>
                    <AlertDialog.Icon status="danger" />
                    <AlertDialog.Header>
                      <AlertDialog.Heading>确认删除任务？</AlertDialog.Heading>
                    </AlertDialog.Header>
                    <AlertDialog.Body>
                      确定要删除课程「{taskToDelete?.course.title}
                      」的运行任务吗？如果该任务正在运行将会立即停止。已记录的学情进度不会丢失，之后仍可从课程列表重新添加。
                    </AlertDialog.Body>
                    <AlertDialog.Footer>
                      <AlertDialog.CloseTrigger />
                      <Button
                        variant="danger"
                        isDisabled={busy}
                        onPress={async () => {
                          if (!taskToDelete) return;
                          const cid = taskToDelete.course.id;
                          const title = taskToDelete.course.title;
                          setTaskToDelete(null);
                          const ok = await action("control_uooc_tasks", {
                            courseIds: [cid],
                            action: "delete",
                          });
                          if (ok) {
                            toast.success(`已删除课程「${title}」的任务`);
                          } else {
                            toast.danger("删除任务失败，请重试");
                          }
                        }}
                      >
                        确认删除
                      </Button>
                    </AlertDialog.Footer>
                  </AlertDialog.Dialog>
                </AlertDialog.Container>
              </AlertDialog.Backdrop>
            </AlertDialog>
            <AlertDialog isOpen={confirmDeleteCompleted} onOpenChange={setConfirmDeleteCompleted}>
              <AlertDialog.Backdrop>
                <AlertDialog.Container size="sm">
                  <AlertDialog.Dialog>
                    <AlertDialog.Icon status="danger" />
                    <AlertDialog.Header>
                      <AlertDialog.Heading>确认删除所有已完成任务？</AlertDialog.Heading>
                    </AlertDialog.Header>
                    <AlertDialog.Body>
                      将从任务列表中移除所有状态为「已完成」的课程任务。已记录的学情进度不会丢失。
                    </AlertDialog.Body>
                    <AlertDialog.Footer>
                      <AlertDialog.CloseTrigger />
                      <Button
                        variant="danger"
                        isDisabled={busy}
                        onPress={async () => {
                          setConfirmDeleteCompleted(false);
                          const completedIds = tasks
                            .filter((t) => t.status === "completed")
                            .map((t) => t.course.id);
                          if (!completedIds.length) return;
                          const ok = await action("control_uooc_tasks", {
                            courseIds: completedIds,
                            action: "delete",
                          });
                          if (ok) {
                            toast.success(`已删除 ${completedIds.length} 个已完成任务`);
                          } else {
                            toast.danger("删除任务失败，请重试");
                          }
                        }}
                      >
                        确认删除
                      </Button>
                    </AlertDialog.Footer>
                  </AlertDialog.Dialog>
                </AlertDialog.Container>
              </AlertDialog.Backdrop>
            </AlertDialog>
          </Tabs.Panel>
          <Tabs.Panel id="settings" className="grid gap-4 pt-4">
            <Card>
              <Card.Header>
                <Card.Title>运行设置</Card.Title>
                <Card.Description>所有课程共享并发额度。请暂停全部任务后修改。</Card.Description>
              </Card.Header>
              <Card.Content className="grid gap-5">
                <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
                  {(
                    [
                      { key: "concurrency", label: "视频并发数", min: 1, step: 1 },
                      { key: "reportIntervalSec", label: "上报间隔（秒）", min: 1, step: 1 },
                      { key: "speedMultiplier", label: "目标倍率", min: 0.1, step: 0.1 },
                      {
                        key: "discussionIntervalMin",
                        label: "讨论间隔（分钟）",
                        min: 1,
                        max: 1440,
                        step: 1,
                      },
                    ] as const
                  ).map(({ key, label, min, step }) => (
                    <NumberField
                      key={key}
                      minValue={min}
                      step={step}
                      value={settings[key] ?? 30}
                      maxValue={key === "discussionIntervalMin" ? 1440 : undefined}
                      isDisabled={running || busy || !snapshot}
                      onChange={(value) => setSettings((old) => ({ ...old, [key]: value }))}
                    >
                      <Label>{label}</Label>
                      <NumberField.Group>
                        <NumberField.DecrementButton />
                        <NumberField.Input />
                        <NumberField.IncrementButton />
                      </NumberField.Group>
                    </NumberField>
                  ))}
                </div>
                <Checkbox
                  isSelected={settings.dryRun}
                  isDisabled={running || busy || !snapshot}
                  onChange={(value) => setSettings((old) => ({ ...old, dryRun: value }))}
                >
                  <Checkbox.Content>
                    <Checkbox.Control>
                      <Checkbox.Indicator />
                    </Checkbox.Control>
                    仅预览（dry-run），不提交视频进度
                  </Checkbox.Content>
                </Checkbox>
                <p className="text-sm text-muted">
                  视频时长成功获取后会保存在本机，后续任务和重启后均直接复用。仅处理视频，测验、作业与考试需自行完成。
                </p>
                <div className="grid gap-3 rounded-xl border border-separator/60 p-4">
                  <Switch
                    isSelected={discussionEnabled}
                    isDisabled={busy || !snapshot}
                    onChange={(value) =>
                      setSettings((old) => ({ ...old, discussionEnabled: value }))
                    }
                  >
                    <Switch.Control>
                      <Switch.Thumb />
                    </Switch.Control>
                    <Switch.Content>
                      <Label>自动完成讨论</Label>
                      <span className="text-xs text-muted">
                        仅处理有讨论权重且完成率未达 100% 的课程，课程之间至少间隔{" "}
                        {discussionIntervalMin} 分钟。
                      </span>
                    </Switch.Content>
                  </Switch>
                  <div className="grid gap-2">
                    <Label htmlFor="uooc-discussion-content">无评论时的发帖内容</Label>
                    <TextArea
                      id="uooc-discussion-content"
                      value={discussionContent}
                      rows={3}
                      maxLength={500}
                      variant="secondary"
                      disabled={busy || !snapshot || !discussionEnabled}
                      onChange={(event) =>
                        setSettings((old) => ({ ...old, discussionContent: event.target.value }))
                      }
                    />
                    <span className="text-xs text-muted">
                      发送时会自动包裹为一段 HTML，主题固定为“要好好学习”。
                    </span>
                  </div>
                  {cooldownText && <p className="text-xs text-muted">{cooldownText}</p>}
                </div>
                <Button
                  className="justify-self-start"
                  isDisabled={running || busy || !snapshot}
                  onPress={() => void action("save_uooc_settings", { settings })}
                >
                  保存设置
                </Button>
              </Card.Content>
            </Card>
            <AiSettings
              onMode={setDefaultMode}
              onOpenAiSettings={() => void navigation.push("/settings")}
            />
          </Tabs.Panel>
          <Tabs.Panel id="qa" className="grid gap-6 pt-4">
            <UoocQaSettings
              courses={snapshot?.courses ?? []}
              quizzes={snapshot?.quizzes}
              qaServerUrl={settings.qaServerUrl ?? "http://localhost:4321"}
              onQaServerUrlChange={(value) =>
                setSettings((old) => ({ ...old, qaServerUrl: value }))
              }
              debugEnabled={debugEnabled}
              disabled={running || busy || !snapshot}
              onSaveSettings={async () => {
                const ok = await action("save_uooc_settings", { settings });
                if (ok) toast.success("题库服务设置已保存");
              }}
            />
          </Tabs.Panel>
        </Tabs>
      </div>
    </PageShell>
  );
}
