import { useEffect, useState } from "react";
import { useSearch } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Alert,
  Button,
  Card,
  Chip,
  Checkbox,
  CloseButton,
  Label,
  NumberField,
  ProgressBar,
  Spinner,
  Tabs,
} from "@heroui/react";
import { LuArrowLeft, LuCheck, LuPause, LuPlay, LuRefreshCw, LuSquare } from "react-icons/lu";
import { PageShell } from "../components";
import { useLogicalNavigation } from "../logical-navigation";
import { CourseProgressView } from "./CourseProgressView";
import {
  isUoocSnapshot,
  newerSnapshot,
  taskStatus,
  checkinActive,
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
  const { tab } = useSearch({ from: "/uooc" });
  const [snapshot, setSnapshot] = useState<UoocSnapshot>();
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [selected, setSelected] = useState<string[]>([]);
  const [settings, setSettings] = useState<UoocSettings>({
    concurrency: 3,
    reportIntervalSec: 20,
    speedMultiplier: 2,
    dryRun: false,
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

  const saved = snapshot?.settings;
  useEffect(() => {
    if (saved) setSettings({ ...saved });
    // Only actual settings changes replace edits, not progress events.
  }, [saved?.concurrency, saved?.reportIntervalSec, saved?.speedMultiplier, saved?.dryRun]);

  const action = async (command: string, args?: Record<string, unknown>) => {
    setBusy(true);
    setError(undefined);
    try {
      const next = await invoke<unknown>(command, args);
      if (isUoocSnapshot(next)) apply(next);
      return true;
    } catch (cause) {
      setError(typeof cause === "string" ? cause : "操作未完成，请重试。");
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
      .catch(() => setError("课程详情刷新失败，请重试。"));
  };
  const tasks =
    snapshot?.tasks.filter((task) => !snapshot.user || task.accountId === snapshot.user.id) ?? [];
  const running = tasks.some(active);
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
  const control = (actionName: "pause" | "resume" | "stop", courseIds: string[] = []) =>
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
            {state.visible ? "手动签到" : "自动签到"}：{state.message}
          </p>
        )}
      </div>
    );
  };

  return (
    <PageShell>
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
        {error && (
          <Alert status="danger" role="alert">
            <Alert.Indicator />
            <Alert.Content>
              <Alert.Title>操作未完成</Alert.Title>
              <Alert.Description>{error}</Alert.Description>
            </Alert.Content>
          </Alert>
        )}
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
              <Tabs.Tab id="settings">
                运行设置
                <Tabs.Indicator />
              </Tabs.Tab>
            </Tabs.List>
          </Tabs.ListContainer>
          <Tabs.Panel id="courses" className="grid gap-4 pt-4">
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
                  variant="secondary"
                  isDisabled={!ready || busy || pendingCheckins.length === 0}
                  onPress={() => void action("start_uooc_checkin", { courseIds: pendingCheckins })}
                >
                  签到全部待签到课程 ({pendingCheckins.length})
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
            {snapshot?.courses.map((course) => (
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
                </Card.Content>
              </Card>
            ))}
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
            </div>
            <p className="text-sm text-muted">
              离开本页后任务继续运行。重启时自动恢复上次运行的课程；主动暂停或停止的任务不会自动恢复。
            </p>
            {tasks.map((task) => (
              <Card key={task.course.id}>
                <Card.Header>
                  <Card.Title>{task.course.title}</Card.Title>
                  {(task.status === "completed" ||
                    snapshot?.progress?.[task.course.id]?.data?.video.percent === 100) && (
                    <Chip color="success" className="justify-self-start">
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
                      继续
                    </Button>
                    <Button
                      size="sm"
                      variant="tertiary"
                      isDisabled={busy || task.status === "stopped"}
                      onPress={() => control("stop", [task.course.id])}
                    >
                      停止
                    </Button>
                  </div>
                  {task.message && (
                    <p role="status" className="text-sm text-muted">
                      {task.message}
                    </p>
                  )}
                  <div className="grid max-h-96 gap-4 overflow-auto">
                    {task.videos.map((video) => (
                      <div key={video.resourceId} className="grid gap-1">
                        <div className="flex flex-wrap justify-between gap-2 text-sm">
                          <span>{video.title}</span>
                          {video.status === "completed" ? (
                            <Chip color="success" size="sm">
                              <LuCheck aria-hidden="true" />
                              <Chip.Label>已刷完</Chip.Label>
                            </Chip>
                          ) : (
                            <span className="text-muted">{taskStatus[video.status]}</span>
                          )}
                        </div>
                        <ProgressBar
                          aria-label={`${video.title}进度`}
                          value={
                            video.status === "completed"
                              ? 100
                              : video.duration
                                ? Math.min(100, (video.position / video.duration) * 100)
                                : 0
                          }
                          color={video.status === "completed" ? "success" : "accent"}
                          isIndeterminate={!video.duration && video.status === "running"}
                        >
                          <ProgressBar.Track>
                            <ProgressBar.Fill />
                          </ProgressBar.Track>
                        </ProgressBar>
                        <p className="text-xs text-muted">
                          {seconds(video.position)} /{" "}
                          {video.duration
                            ? seconds(video.duration)
                            : video.status === "completed"
                              ? "时长未记录"
                              : "等待时长"}
                          {video.message && ` · ${video.message}`}
                        </p>
                      </div>
                    ))}
                  </div>
                </Card.Content>
              </Card>
            ))}
            {!tasks.length && (
              <Card>
                <Card.Content>选择课程并开始后，任务会显示在这里。</Card.Content>
              </Card>
            )}
            {!!snapshot?.logs.length && (
              <Card>
                <Card.Header>
                  <Card.Title>运行记录</Card.Title>
                </Card.Header>
                <Card.Content>
                  <div role="log" className="max-h-48 overflow-auto text-sm text-muted">
                    {snapshot.logs.map((line, index) => (
                      <p key={`${index}-${line}`}>{line}</p>
                    ))}
                  </div>
                </Card.Content>
              </Card>
            )}
          </Tabs.Panel>
          <Tabs.Panel id="settings" className="pt-4">
            <Card>
              <Card.Header>
                <Card.Title>运行设置</Card.Title>
                <Card.Description>所有课程共享并发额度。请暂停全部任务后修改。</Card.Description>
              </Card.Header>
              <Card.Content className="grid gap-5">
                <div className="grid gap-4 sm:grid-cols-3">
                  {(
                    [
                      { key: "concurrency", label: "视频并发数", min: 1, step: 1 },
                      { key: "reportIntervalSec", label: "上报间隔（秒）", min: 1, step: 1 },
                      { key: "speedMultiplier", label: "目标倍率", min: 0.1, step: 0.1 },
                    ] as const
                  ).map(({ key, label, min, step }) => (
                    <NumberField
                      key={key}
                      minValue={min}
                      step={step}
                      value={settings[key]}
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
                <Button
                  className="justify-self-start"
                  isDisabled={running || busy || !snapshot}
                  onPress={() => void action("save_uooc_settings", { settings })}
                >
                  保存设置
                </Button>
              </Card.Content>
            </Card>
          </Tabs.Panel>
        </Tabs>
      </div>
    </PageShell>
  );
}
