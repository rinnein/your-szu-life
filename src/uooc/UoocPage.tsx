import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useSearch } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createTaskChannel, useChannelSubscription } from "../tauri-channel";
import { Dialog, LoadingIndicator } from "material/react";
import { ActionMenu, AdaptiveTabs, Banner, toast } from "../ui";
import { Button } from "material/react";
import { LuEllipsisVertical, LuRefreshCw } from "react-icons/lu";
import { PageShell, SkeletonCard } from "../components";
import { TopAppBar } from "../shell/TopAppBar";
import { useLogicalNavigation } from "../logical-navigation";
import { usePending } from "../use-pending";
import { errorMessage } from "../async-state";
import { clearPostponed, isPostponed, postpone } from "../reminder-postpone";
import { ModelDownloadModal } from "../ModelDownloadModal";
import { useEmbeddingModelStatus } from "../EmbeddingModelSettings";
import { CoursesTab } from "./courses/CoursesTab";
import { AssessmentsTab } from "./assessments/AssessmentsTab";
import { BankTab } from "./bank/BankTab";
import { SettingsTab } from "./settings/SettingsTab";
import { QuizPanel } from "./quiz/QuizPanel";
import { taskActive } from "./state/status";
import {
  isUoocSnapshot,
  newerSnapshot,
  type UoocAiConfig,
  type QuizMode,
  type QuizState,
  type UoocSnapshot,
  type UoocTask,
} from "./state/types";
import { DEFAULT_QA_SERVER_URL } from "./state/constants";

/** 向量模型提醒在 localStorage 里的推迟记录键。 */
export const EMBEDDING_REMINDER_KEY = "embedding_model_reminder_postponed_until";

/**
 * 题库与课程进度都不是页面本地状态。
 *
 * - 题库已有内容时绝不能重建：重建会重置 uploaded 标记，并在新解析缺答案时
 *   覆盖已确认的标准答案。
 * - 进度数据只在应用重启后为空，同一会话里重复打开页面不需要再全量刷新。
 *
 * 这两处都用模块级时间戳，组件反复卸载/挂载不会重置它们。
 */
const PROGRESS_REFRESH_TTL_MS = 10 * 60 * 1000;
const BANK_CHECK_TTL_MS = 60 * 1000;
let progressRefreshedAt = 0;
let bankCheckedAt = 0;

async function ensureLocalBank(): Promise<void> {
  if (Date.now() - bankCheckedAt < BANK_CHECK_TTL_MS) return;
  bankCheckedAt = Date.now();
  const count = await invoke<number>("get_uooc_local_bank_count").catch(() => -1);
  // 读取失败（-1）或已有题目（>0）时都不要重建。
  if (count !== 0) return;
  await invoke("build_uooc_local_bank", {
    onProgress: createTaskChannel(() => {}),
  }).catch(() => {});
}

async function refreshProgressIfStale(apply: (snapshot: UoocSnapshot) => void): Promise<void> {
  if (Date.now() - progressRefreshedAt < PROGRESS_REFRESH_TTL_MS) return;
  const progress = await invoke<UoocSnapshot>("refresh_uooc_progress", {
    courseIds: [],
  }).catch(() => null);
  if (progress && isUoocSnapshot(progress)) {
    // 只有成功拿到进度才记时间，失败时下次打开页面仍会重试。
    progressRefreshedAt = Date.now();
    apply(progress);
  }
}

/** 仅用于测试：重置模块级缓存时间戳，避免用例之间互相影响。 */
export function resetUoocPageCaches(): void {
  progressRefreshedAt = 0;
  bankCheckedAt = 0;
}

const sessionBanner: Record<
  UoocSnapshot["session"],
  { title: string; tone: "default" | "warning" | "danger" }
> = {
  checking: { title: "正在检查登录状态", tone: "default" },
  ready: { title: "已登录", tone: "default" },
  loginRequired: { title: "需要登录", tone: "warning" },
  networkError: { title: "网络连接失败", tone: "danger" },
  loggingIn: { title: "等待官方登录完成", tone: "default" },
};

/** 页面级弹窗：同一时刻最多一个。 */
export type PageDialog =
  | { kind: "none" }
  | { kind: "quiz"; quiz: QuizState }
  | { kind: "deleteTask"; task: UoocTask }
  | { kind: "deleteCompleted" }
  | { kind: "logout"; busy: boolean };

export default function UoocPage() {
  const navigation = useLogicalNavigation();
  const { tab, subtab, course } = useSearch({ from: "/study/uooc" });
  const [snapshot, setSnapshot] = useState<UoocSnapshot>();
  const [connectError, setConnectError] = useState<string>();
  const [dialog, setDialog] = useState<PageDialog>({ kind: "none" });
  const [defaultMode, setDefaultMode] = useState<QuizMode>("supervised");
  const [debugEnabled, setDebugEnabled] = useState(false);
  const [accountMenuOpen, setAccountMenuOpen] = useState(false);
  const embeddingStatus = useEmbeddingModelStatus();
  const [embeddingModalOpen, setEmbeddingModalOpen] = useState(false);
  const pending = usePending();

  // useCallback 稳定引用：apply 出现在 bootstrap / ready 初始化两个 effect 的依赖里，
  // 每次渲染都换新引用会让 effect 反复重跑、互相取消（快照永远落不了地）。
  const apply = useCallback(
    (next: UoocSnapshot) => setSnapshot((old) => newerSnapshot(old, next)),
    [],
  );

  useChannelSubscription<{ type: "initial" | "snapshot"; data: UoocSnapshot }>(
    "subscribe_uooc_state",
    (event) => {
      if (isUoocSnapshot(event.data)) {
        apply(event.data);
      }
    },
    {},
    {
      onError: (cause) =>
        setConnectError(errorMessage(cause, "无法连接 UOOC 状态通道，请在原生应用中重试。")),
    },
  );

  useEffect(() => {
    const signal = { cancelled: false };
    let stop: (() => void) | undefined;
    void listen<UoocSnapshot>("uooc-state-changed", ({ payload }) => {
      if (!signal.cancelled && isUoocSnapshot(payload)) apply(payload);
    })
      .then(async (dispose) => {
        if (signal.cancelled) {
          dispose();
          return;
        }
        stop = dispose;
        const next = await invoke<UoocSnapshot>("get_uooc_snapshot");
        if (signal.cancelled) return;
        if (isUoocSnapshot(next)) {
          setConnectError(undefined);
          apply(next);
        } else {
          setConnectError("请在泥深原生应用中使用 UOOC 功能。");
        }
      })
      .catch((cause: unknown) => {
        if (signal.cancelled) return;
        setConnectError(errorMessage(cause, "无法读取 UOOC 服务，请在原生应用中重试。"));
      });
    return () => {
      signal.cancelled = true;
      stop?.();
    };
  }, [apply]);

  // 会话就绪后（首次或官方登录完成）做一次题库与进度的懒初始化。
  const ready = snapshot?.session === "ready";
  const bootstrappedForReady = useRef(false);
  useEffect(() => {
    if (!ready) {
      bootstrappedForReady.current = false;
      return;
    }
    if (bootstrappedForReady.current) return;
    bootstrappedForReady.current = true;
    void ensureLocalBank();
    void refreshProgressIfStale(apply);
  }, [ready, apply]);

  useEffect(() => {
    void invoke<UoocAiConfig>("get_uooc_ai_config")
      .then((v) => {
        if (v?.defaultMode) setDefaultMode(v.defaultMode);
      })
      .catch(() => {});
  }, []);

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

  // 向量模型提醒：进入页面时弹出；任何方式关闭都推迟 1 天，就绪后清除推迟记录。
  useEffect(() => {
    if (
      embeddingStatus &&
      embeddingStatus.status !== "ready" &&
      embeddingStatus.status !== "checking" &&
      !isPostponed(EMBEDDING_REMINDER_KEY)
    ) {
      setEmbeddingModalOpen(true);
    }
  }, [embeddingStatus?.status]);

  useEffect(() => {
    if (embeddingStatus?.status === "ready") {
      setEmbeddingModalOpen(false);
      clearPostponed(EMBEDDING_REMINDER_KEY);
    }
  }, [embeddingStatus?.status]);

  const closeEmbeddingModal = (open: boolean) => {
    setEmbeddingModalOpen(open);
    if (!open && embeddingStatus && embeddingStatus.status !== "ready") {
      postpone(EMBEDDING_REMINDER_KEY);
    }
  };

  // 深链接：?course=<cid> 定位课程卡片并自动打开对应测验弹窗。
  const tasks = useMemo(
    () =>
      snapshot?.tasks.filter((task) => !snapshot.user || task.accountId === snapshot.user.id) ?? [],
    [snapshot],
  );
  useEffect(() => {
    if (!course) return;
    document.getElementById(`uooc-course-${course}`)?.scrollIntoView?.({ block: "start" });
    const quiz = Object.values(snapshot?.quizzes ?? {}).find(
      (q) => q.cid === course && (!snapshot?.user || q.accountId === snapshot.user.id),
    );
    if (quiz) setDialog({ kind: "quiz", quiz });
  }, [tab, course, snapshot]);

  /**
   * 统一入口的页面动作：按键防重（同键重复触发直接忽略），成功吸收快照，
   * 失败只提示一次。key 用「域:目标」的形状，如 `session`、`task:<cid>`。
   */
  const action = async (key: string, command: string, args?: Record<string, unknown>) => {
    const outcome = await pending.run(key, () => invoke<unknown>(command, args));
    if (outcome.ok) {
      if (isUoocSnapshot(outcome.value)) apply(outcome.value);
      return true;
    }
    if (outcome.skipped) return false;
    toast.danger("操作失败", { description: errorMessage(outcome.error) });
    return false;
  };

  const requestLogout = async () => {
    const outcome = await pending.run("session", async () => {
      const sync = await invoke<{
        enabled: boolean;
        items: { id: string; state: string }[];
      }>("get_sync_status");
      const item = sync.items.find((entry) => entry.id === "uooc.session");
      return !sync.enabled || item?.state === "disabled"
        ? ("direct" as const)
        : ("confirm" as const);
    });
    if (outcome.ok) {
      if (outcome.value === "direct") {
        const ok = await action("session", "logout_uooc");
        if (ok) toast.success("已退出 UOOC");
        return;
      }
      setDialog({ kind: "logout", busy: false });
      return;
    }
    if (outcome.skipped) return;
    // 读不到同步状态时退回普通退出，不让退出被同步模块阻塞。
    const ok = await action("session", "logout_uooc");
    if (ok) toast.success("已退出 UOOC");
  };

  const confirmLogout = async (mode: "global" | "local") => {
    setDialog({ kind: "logout", busy: true });
    try {
      await invoke("logout_uooc_with_mode", { mode });
      setDialog({ kind: "none" });
      toast.success(mode === "global" ? "已全局退出 UOOC" : "已在本机退出 UOOC", {
        description:
          mode === "global"
            ? "正在通知其它已连接设备退出登录。"
            : "本机已停止同步 UOOC 登录态，其它设备不受影响。",
      });
    } catch (cause) {
      setDialog({ kind: "logout", busy: false });
      toast.danger("退出 UOOC 失败", { description: errorMessage(cause, "退出失败，请重试。") });
    }
  };

  const refreshSession = async () => {
    const ok = await action("session", "refresh_uooc_session");
    if (ok) toast.success("已刷新课程");
  };

  const openQuiz = (quiz: QuizState) => setDialog({ kind: "quiz", quiz });
  const closeDialog = () => setDialog({ kind: "none" });
  const requestDialog = (
    next: { kind: "deleteTask"; task: UoocTask } | { kind: "deleteCompleted" },
  ) => setDialog(next);

  const activeModalQuiz =
    dialog.kind === "quiz"
      ? (Object.values(snapshot?.quizzes ?? {}).find(
          (q) =>
            q.cid === dialog.quiz.cid &&
            (!snapshot?.user || q.accountId === snapshot.user.id) &&
            ((q.taskId && q.taskId === dialog.quiz.taskId) ||
              (q.resourceId && q.resourceId === dialog.quiz.resourceId)),
        ) ?? dialog.quiz)
      : null;

  const tabItems = useMemo(
    () => [
      { value: "courses", label: "课程" },
      { value: "exams", label: "考核" },
      { value: "qa", label: "题库" },
      { value: "settings", label: "设置" },
    ],
    [],
  );

  const sessionInfo = snapshot ? sessionBanner[snapshot.session] : undefined;
  const sessionNeedsAttention =
    !!sessionInfo && (sessionInfo.tone === "warning" || sessionInfo.tone === "danger");
  const loggedIn = Boolean(snapshot?.user);
  const deleting = pending.isPending("delete-tasks");

  return (
    <PageShell maxWidth="lg">
      <ModelDownloadModal
        kind="embedding"
        isOpen={embeddingModalOpen}
        onOpenChange={closeEmbeddingModal}
      />
      <TopAppBar
        title="UOOC刷课"
        description="优课在线视频任务"
        actions={
          <div className="flex items-center gap-1">
            <button
              type="button"
              data-webview-navigation
              className="top-app-bar-icon-button"
              aria-label="刷新课程"
              title="刷新课程"
              disabled={pending.isPending("session")}
              onClick={() => void refreshSession()}
            >
              {pending.isPending("session") ? (
                <LoadingIndicator size={20} />
              ) : (
                <LuRefreshCw aria-hidden="true" size={20} />
              )}
            </button>
            {loggedIn && (
              <button
                type="button"
                id="uooc-account-trigger"
                data-webview-navigation
                className="top-app-bar-icon-button"
                aria-label="打开账户菜单"
                aria-haspopup="menu"
                aria-expanded={accountMenuOpen}
                onClick={() => setAccountMenuOpen(true)}
              >
                <LuEllipsisVertical aria-hidden="true" size={20} />
              </button>
            )}
            {loggedIn && (
              <ActionMenu
                anchorId="uooc-account-trigger"
                open={accountMenuOpen}
                onOpenChange={setAccountMenuOpen}
                ariaLabel="账户操作"
                items={[
                  { kind: "note", label: snapshot?.user?.name ?? "" },
                  { kind: "divider" },
                  { kind: "item", value: "login", label: "恢复官方登录" },
                  { kind: "item", value: "logout", label: "退出 UOOC" },
                ]}
                onSelect={(value) => {
                  if (value === "login") void action("session", "open_uooc_login");
                  if (value === "logout") void requestLogout();
                }}
              />
            )}
          </div>
        }
      />

      {connectError && (
        <Banner status="danger" role="alert" className="mb-4">
          <Banner.Title>无法读取 UOOC 服务</Banner.Title>
          <Banner.Description>{connectError}</Banner.Description>
          <Button
            variant="tonal"
            className="mt-2 self-start"
            onClick={() => {
              setConnectError(undefined);
              // 重试走一遍初始引导：重新订阅事件 + 拉取快照。
              void (async () => {
                try {
                  const next = await invoke<UoocSnapshot>("get_uooc_snapshot");
                  if (isUoocSnapshot(next)) {
                    apply(next);
                  } else {
                    setConnectError("请在泥深原生应用中使用 UOOC 功能。");
                  }
                } catch (cause) {
                  setConnectError(errorMessage(cause, "无法读取 UOOC 服务，请在原生应用中重试。"));
                }
              })();
            }}
          >
            重试
          </Button>
        </Banner>
      )}
      {!connectError && !snapshot && <SkeletonCard />}
      {sessionNeedsAttention && sessionInfo && (
        <Banner
          status={sessionInfo.tone === "danger" ? "danger" : "warning"}
          role={sessionInfo.tone === "danger" ? "alert" : "status"}
          className="mb-4"
        >
          <Banner.Title>{sessionInfo.title}</Banner.Title>
          <Banner.Description>
            {snapshot?.message || "登录后才能看到课程、任务与考核。"}
          </Banner.Description>
          <Button
            variant="tonal"
            className="mt-2 self-start"
            disabled={pending.isPending("session")}
            onClick={() => void action("session", "open_uooc_login")}
          >
            登录 UOOC
          </Button>
        </Banner>
      )}

      {snapshot && (
        <>
          <AdaptiveTabs
            label="功能导航"
            items={tabItems}
            value={tab}
            onChange={(next) => void navigation.replace(`/study/uooc?tab=${next}`)}
            ariaLabel="UOOC 功能"
            className="mb-2"
          />
          {tab === "courses" && (
            <CoursesTab
              snapshot={snapshot}
              tasks={tasks}
              ready={ready}
              defaultMode={defaultMode}
              action={action}
              openQuiz={openQuiz}
              requestDialog={requestDialog}
            />
          )}
          {tab === "exams" && (
            <AssessmentsTab
              snapshot={snapshot}
              ready={ready}
              subtab={subtab}
              onSubtabChange={(next) =>
                void navigation.replace(`/study/uooc?tab=exams&subtab=${next}`)
              }
              openQuiz={openQuiz}
              onSnapshot={apply}
            />
          )}
          {tab === "qa" && (
            <BankTab
              snapshot={snapshot}
              ready={ready}
              debugEnabled={debugEnabled}
              qaServerUrl={snapshot.settings.qaServerUrl ?? DEFAULT_QA_SERVER_URL}
              action={action}
            />
          )}
          {tab === "settings" && (
            <SettingsTab
              snapshot={snapshot}
              ready={ready}
              action={action}
              onMode={setDefaultMode}
            />
          )}
        </>
      )}

      {activeModalQuiz &&
        typeof document !== "undefined" &&
        createPortal(
          <Dialog
            open
            role="dialog"
            headline={activeModalQuiz.title || "课程测验"}
            aria-label={activeModalQuiz.title || "课程测验"}
            onClose={closeDialog}
          >
            <div className="flex flex-col max-h-[85vh] overflow-y-auto">
              <QuizPanel
                quiz={activeModalQuiz}
                active={tasks.some((t) => t.course.id === activeModalQuiz.cid && taskActive(t))}
                weight={snapshot?.progress?.[activeModalQuiz.cid]?.data?.test.weight}
                dryRun={snapshot?.settings.dryRun ?? false}
                onSnapshot={apply}
                onClose={closeDialog}
              />
            </div>
          </Dialog>,
          document.body,
        )}
      {(dialog.kind === "deleteTask" || dialog.kind === "deleteCompleted") &&
        typeof document !== "undefined" &&
        createPortal(
          <Dialog
            open
            headline={dialog.kind === "deleteTask" ? "确认删除任务？" : "确认删除所有已完成任务？"}
            onClose={closeDialog}
          >
            {dialog.kind === "deleteTask" ? (
              <div>
                确定要删除课程「{dialog.task.course.title}
                」的运行任务吗？如果该任务正在运行将会立即停止。已记录的学情进度不会丢失，之后仍可从课程列表重新添加。
              </div>
            ) : (
              <div>
                将从任务列表中移除所有状态为「已完成」的课程任务。已记录的学情进度不会丢失。
              </div>
            )}
            <div slot="actions">
              <Button variant="tonal" onClick={closeDialog}>
                取消
              </Button>
              <Button
                className="button-danger"
                variant="filled"
                disabled={deleting}
                onClick={async () => {
                  const isSingle = dialog.kind === "deleteTask";
                  const courseIds = isSingle
                    ? [dialog.task.course.id]
                    : tasks.filter((t) => t.status === "completed").map((t) => t.course.id);
                  const title = isSingle ? dialog.task.course.title : "";
                  if (isSingle) closeDialog();
                  const ok = await action("delete-tasks", "control_uooc_tasks", {
                    courseIds,
                    action: "delete",
                  });
                  if (ok) {
                    if (!isSingle) closeDialog();
                    toast.success(
                      isSingle
                        ? `已删除课程「${title}」的任务`
                        : `已删除 ${courseIds.length} 个已完成任务`,
                    );
                  } else if (!isSingle) {
                    closeDialog();
                  }
                }}
              >
                确认删除
              </Button>
            </div>
          </Dialog>,
          document.body,
        )}
      {dialog.kind === "logout" &&
        typeof document !== "undefined" &&
        createPortal(
          <Dialog
            open
            headline="退出 UOOC 登录"
            onClose={() => {
              if (!dialog.busy) setDialog({ kind: "none" });
            }}
          >
            <div>
              设备同步已开启。全局退出会让所有同账号设备退出 UOOC 登录；仅本机退出会停止本机同步
              UOOC 登录态，不影响其它设备。
            </div>
            <div slot="actions">
              <Button
                variant="text"
                disabled={dialog.busy}
                onClick={() => setDialog({ kind: "none" })}
              >
                取消
              </Button>
              <Button
                variant="tonal"
                disabled={dialog.busy}
                onClick={() => void confirmLogout("local")}
              >
                {dialog.busy && <LoadingIndicator size={16} />}
                仅本机退出
              </Button>
              <Button
                className="button-danger"
                variant="filled"
                disabled={dialog.busy}
                onClick={() => void confirmLogout("global")}
              >
                全局退出
              </Button>
            </div>
          </Dialog>,
          document.body,
        )}
    </PageShell>
  );
}
